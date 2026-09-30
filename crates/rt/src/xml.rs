//! The events XML PARSE reports under XMLPARSE(XMLSS) (Language Reference SC27-8713-03, pp. 29-33,
//! 489-494; Programming Guide SC27-8714-03, pp. 649-653). The document may arrive in segments: markup
//! is held until it is complete, while character content, comments and processing-instruction data
//! that a segment ends inside are reported in parts.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    StartOfDocument,
    VersionInformation,
    EncodingDeclaration,
    StandaloneDeclaration,
    DocumentTypeDeclaration,
    Comment,
    ProcessingInstructionTarget,
    ProcessingInstructionData,
    StartOfElement,
    NamespaceDeclaration,
    AttributeName,
    AttributeCharacters,
    AttributeNationalCharacter,
    ContentCharacters,
    ContentNationalCharacter,
    StartOfCdataSection,
    EndOfCdataSection,
    EndOfElement,
    UnresolvedReference,
    EndOfInput,
    EndOfDocument,
    Exception,
}

impl EventKind {
    /// The XML-EVENT value.
    pub fn name(self) -> &'static str {
        match self {
            Self::StartOfDocument => "START-OF-DOCUMENT",
            Self::VersionInformation => "VERSION-INFORMATION",
            Self::EncodingDeclaration => "ENCODING-DECLARATION",
            Self::StandaloneDeclaration => "STANDALONE-DECLARATION",
            Self::DocumentTypeDeclaration => "DOCUMENT-TYPE-DECLARATION",
            Self::Comment => "COMMENT",
            Self::ProcessingInstructionTarget => "PROCESSING-INSTRUCTION-TARGET",
            Self::ProcessingInstructionData => "PROCESSING-INSTRUCTION-DATA",
            Self::StartOfElement => "START-OF-ELEMENT",
            Self::NamespaceDeclaration => "NAMESPACE-DECLARATION",
            Self::AttributeName => "ATTRIBUTE-NAME",
            Self::AttributeCharacters => "ATTRIBUTE-CHARACTERS",
            Self::AttributeNationalCharacter => "ATTRIBUTE-NATIONAL-CHARACTER",
            Self::ContentCharacters => "CONTENT-CHARACTERS",
            Self::ContentNationalCharacter => "CONTENT-NATIONAL-CHARACTER",
            Self::StartOfCdataSection => "START-OF-CDATA-SECTION",
            Self::EndOfCdataSection => "END-OF-CDATA-SECTION",
            Self::EndOfElement => "END-OF-ELEMENT",
            Self::UnresolvedReference => "UNRESOLVED-REFERENCE",
            Self::EndOfInput => "END-OF-INPUT",
            Self::EndOfDocument => "END-OF-DOCUMENT",
            Self::Exception => "EXCEPTION",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    /// XML-TEXT or XML-NTEXT.
    pub text: String,
    /// XML-NAMESPACE: the identifier bound to the name's prefix, or to the default namespace.
    pub namespace: String,
    /// XML-NAMESPACE-PREFIX.
    pub prefix: String,
    /// XML-INFORMATION: 1 when attribute or content characters are complete, 2 when more follow,
    /// 0 for every other event (p. 34).
    pub information: i32,
}

impl Event {
    fn new(kind: EventKind, text: impl Into<String>) -> Self {
        Self { kind, text: text.into(), namespace: String::new(), prefix: String::new(), information: 0 }
    }

    fn characters(kind: EventKind, text: String, complete: bool) -> Self {
        Self { information: if complete { 1 } else { 2 }, ..Self::new(kind, text) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    UnexpectedEnd,
    InvalidCharacter,
    MismatchedEndTag,
    DuplicateAttribute,
    UndeclaredElementPrefix,
    UndeclaredAttributePrefix,
    UndeclaredEntity,
    BadReference,
    ContentAfterRoot,
    SecondRoot,
    NoRoot,
    BadDeclaration,
    BadComment,
    LessThanInAttribute,
}

impl Why {
    /// XML-CODE: z/OS XML System Services' return code 12 (not well formed) and its reason code,
    /// or return code 4 and Enterprise COBOL's own reason for an undeclared prefix (Programming
    /// Guide SC27-8714-03, pp. 645, 809; z/OS XML System Services User's Guide SA38-0681-50,
    /// Appendix B; assumption C118).
    pub fn code(self) -> i32 {
        let not_well_formed = |reason: i32| 0x000C_0000 | reason;
        match self {
            Why::UnexpectedEnd => not_well_formed(0x2004),
            Why::NoRoot => not_well_formed(0x2019),
            Why::DuplicateAttribute => not_well_formed(0x3000),
            Why::BadComment => not_well_formed(0x3008),
            Why::LessThanInAttribute => not_well_formed(0x3022),
            Why::BadReference => not_well_formed(0x3028),
            Why::MismatchedEndTag => not_well_formed(0x3035),
            Why::BadDeclaration => not_well_formed(0x3060),
            Why::UndeclaredEntity => not_well_formed(0x3061),
            Why::InvalidCharacter | Why::ContentAfterRoot => not_well_formed(0x3062),
            Why::SecondRoot => not_well_formed(0x3065),
            Why::UndeclaredAttributePrefix => 0x0004_0800,
            Why::UndeclaredElementPrefix => 0x0004_0801,
        }
    }
}

/// Where the document stops being well formed: the offset, in characters from the start of the
/// document, of the first character in error, or of its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Malformed {
    pub offset: usize,
    pub why: Why,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Event(Event),
    /// The segment is used up and the document is not finished: END-OF-INPUT.
    EndOfInput,
    Error(Malformed),
    /// END-OF-DOCUMENT has been reported.
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Start,
    Prolog,
    Content,
    Epilogue,
    Finished,
}

/// A construct a segment ended inside, whose text is being reported in parts.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    Comment,
    /// A processing instruction's target, and whether part of its data has been reported: the
    /// target is reported again before each later part (p. 32).
    Instruction(String, bool),
    Cdata,
}

struct Element {
    qualified: String,
    local: String,
    prefix: String,
    namespace: String,
    bindings: usize,
}

pub const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

pub struct Scanner<'r> {
    buf: Vec<char>,
    at: usize,
    consumed: usize,
    place: Place,
    open: Option<Open>,
    elements: Vec<Element>,
    /// Namespace bindings in scope, innermost last: prefix ("" for the default) and identifier.
    bindings: Vec<(String, String)>,
    pending: VecDeque<Event>,
    representable: &'r dyn Fn(char) -> bool,
    last_segment: bool,
    doctype: bool,
    standalone_no: bool,
}

type Scan<T> = Result<T, Why>;

/// Not enough input to finish a construct.
const MORE: Why = Why::UnexpectedEnd;

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':' || c as u32 >= 0xC0
}

fn name_char(c: char) -> bool {
    name_start(c) || c.is_ascii_digit() || c == '-' || c == '.' || c == '\u{B7}'
}

fn xml_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn split(qualified: &str) -> (String, String) {
    match qualified.split_once(':') {
        Some((prefix, local)) => (prefix.to_owned(), local.to_owned()),
        None => (String::new(), qualified.to_owned()),
    }
}

impl<'r> Scanner<'r> {
    /// `representable(c)` says whether the document's code page holds c: a character reference
    /// to one it does not is reported as a NATIONAL-CHARACTER event.
    pub fn new(segment: &str, representable: &'r dyn Fn(char) -> bool) -> Self {
        Self {
            buf: segment.chars().collect(),
            at: 0,
            consumed: 0,
            place: Place::Start,
            open: None,
            elements: Vec::new(),
            bindings: vec![("xml".into(), XML_NAMESPACE.into())],
            pending: VecDeque::new(),
            representable,
            last_segment: false,
            doctype: false,
            standalone_no: false,
        }
    }

    /// The next segment, after END-OF-INPUT, joining what the last one left unfinished.
    pub fn feed(&mut self, segment: &str) {
        self.consumed += self.at;
        self.buf.drain(..self.at);
        self.at = 0;
        self.buf.extend(segment.chars());
    }

    /// No segment follows: what is unfinished is an error.
    pub fn finish(&mut self) {
        self.last_segment = true;
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.buf.get(self.at + ahead).copied()
    }

    fn starts(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        self.buf[self.at..].starts_with(&chars)
    }

    /// Whether the rest of the buffer could still be the start of `s`.
    fn could_start(&self, s: &str) -> bool {
        let rest = &self.buf[self.at..];
        rest.len() < s.chars().count() && s.chars().zip(rest).all(|(a, &b)| a == b)
    }

    fn find(&self, from: usize, s: &str) -> Option<usize> {
        let chars: Vec<char> = s.chars().collect();
        (from..=self.buf.len().saturating_sub(chars.len())).find(|&i| self.buf[i..].starts_with(&chars))
    }

    fn fail(&self, at: usize, why: Why) -> Step {
        Step::Error(Malformed { offset: self.consumed + at, why })
    }

    pub fn advance(&mut self) -> Step {
        if let Some(e) = self.pending.pop_front() {
            return Step::Event(e);
        }
        if self.place == Place::Start {
            self.place = Place::Prolog;
            return Step::Event(Event::new(EventKind::StartOfDocument, ""));
        }
        if self.place == Place::Finished {
            return Step::Done;
        }
        let before = self.at;
        match self.step() {
            Ok(()) => match self.pending.pop_front() {
                Some(e) => Step::Event(e),
                None if self.place == Place::Finished => Step::Event(Event::new(EventKind::EndOfDocument, "")),
                None => self.advance(),
            },
            Err(MORE) => {
                self.at = before;
                self.at_end()
            }
            Err(why) => self.fail(self.at, why),
        }
    }

    /// The segment is used up, or holds only part of a construct.
    fn at_end(&mut self) -> Step {
        if self.place == Place::Epilogue && self.buf[self.at..].iter().all(|&c| is_space(c)) {
            self.at = self.buf.len();
            self.place = Place::Finished;
            return Step::Event(Event::new(EventKind::EndOfDocument, ""));
        }
        if !self.last_segment {
            return Step::EndOfInput;
        }
        let why = if self.place == Place::Prolog && self.open.is_none() && self.buf[self.at..].iter().all(|&c| is_space(c)) { Why::NoRoot } else { Why::UnexpectedEnd };
        Step::Error(Malformed { offset: self.consumed + self.buf.len(), why })
    }

    /// Scans one construct, queueing its events; `Err(MORE)` leaves the cursor where it was.
    fn step(&mut self) -> Scan<()> {
        if let Some(open) = self.open.clone() {
            return self.continue_open(open);
        }
        while self.place != Place::Content && self.peek(0).is_some_and(is_space) {
            self.at += 1;
        }
        let Some(c) = self.peek(0) else { return Err(MORE) };
        if c != '<' {
            if self.place == Place::Content {
                return self.content();
            }
            return Err(if self.place == Place::Epilogue { Why::ContentAfterRoot } else { Why::InvalidCharacter });
        }
        if self.starts("<?xml") && self.peek(5).is_some_and(|c| is_space(c) || c == '?') && self.consumed == 0 && self.at == 0 {
            return self.declaration();
        }
        if self.could_start("<!--") || self.could_start("<![CDATA[") || self.could_start("<!DOCTYPE") || self.buf.len() - self.at < 2 {
            return Err(MORE);
        }
        if self.starts("<!--") {
            self.at += 4;
            self.open = Some(Open::Comment);
            return Ok(());
        }
        if self.starts("<![CDATA[") {
            if self.place != Place::Content {
                return Err(Why::InvalidCharacter);
            }
            if self.peek(9).is_none() {
                return Err(MORE);
            }
            self.at += 9;
            self.pending.push_back(Event::new(EventKind::StartOfCdataSection, ""));
            self.open = Some(Open::Cdata);
            return Ok(());
        }
        if self.starts("<!DOCTYPE") {
            return self.doctype();
        }
        if self.starts("<?") {
            return self.instruction();
        }
        if self.starts("</") {
            return self.end_tag();
        }
        if self.place == Place::Epilogue {
            return Err(Why::SecondRoot);
        }
        self.start_tag()
    }

    fn name(&mut self) -> Scan<String> {
        let start = self.at;
        match self.peek(0) {
            None => return Err(MORE),
            Some(c) if !name_start(c) => return Err(Why::InvalidCharacter),
            _ => {}
        }
        while self.peek(0).is_some_and(name_char) {
            self.at += 1;
        }
        if self.at == self.buf.len() {
            return Err(MORE);
        }
        let name: String = self.buf[start..self.at].iter().collect();
        if name.matches(':').count() > 1 || name.starts_with(':') || name.ends_with(':') {
            self.at = start;
            return Err(Why::InvalidCharacter);
        }
        Ok(name)
    }

    fn spaces(&mut self) -> bool {
        let start = self.at;
        while self.peek(0).is_some_and(is_space) {
            self.at += 1;
        }
        self.at > start
    }

    fn expect(&mut self, c: char) -> Scan<()> {
        match self.peek(0) {
            None => Err(MORE),
            Some(d) if d == c => {
                self.at += 1;
                Ok(())
            }
            Some(_) => Err(Why::InvalidCharacter),
        }
    }

    /// A reference after '&': the character it stands for, or `Err(name)` for an entity that is
    /// not one of the five predefined.
    fn reference(&mut self) -> Scan<Result<char, String>> {
        let end = self.find(self.at, ";").ok_or(MORE)?;
        let body: String = self.buf[self.at + 1..end].iter().collect();
        let value = if let Some(hex) = body.strip_prefix("#x") {
            u32::from_str_radix(hex, 16).ok().filter(|&v| xml_char(v) && !hex.is_empty()).and_then(char::from_u32).map(Ok).ok_or(Why::BadReference)?
        } else if let Some(dec) = body.strip_prefix('#') {
            dec.parse::<u32>().ok().filter(|&v| xml_char(v)).and_then(char::from_u32).map(Ok).ok_or(Why::BadReference)?
        } else {
            match body.as_str() {
                "lt" => Ok('<'),
                "gt" => Ok('>'),
                "amp" => Ok('&'),
                "apos" => Ok('\''),
                "quot" => Ok('"'),
                name if !name.is_empty() && name.chars().next().is_some_and(name_start) && name.chars().all(name_char) => Err(name.to_owned()),
                _ => return Err(Why::BadReference),
            }
        };
        self.at = end + 1;
        Ok(value)
    }

    fn declaration(&mut self) -> Scan<()> {
        let end = self.find(self.at, "?>").ok_or(MORE)?;
        self.at += 5;
        let mut seen = Vec::new();
        loop {
            self.spaces();
            if self.at >= end {
                break;
            }
            let name = self.name().map_err(|_| Why::BadDeclaration)?;
            self.spaces();
            self.expect('=').map_err(|_| Why::BadDeclaration)?;
            self.spaces();
            let quote = self.peek(0).filter(|&q| q == '"' || q == '\'').ok_or(Why::BadDeclaration)?;
            let close = self.find(self.at + 1, &quote.to_string()).filter(|&c| c < end).ok_or(Why::BadDeclaration)?;
            let value: String = self.buf[self.at + 1..close].iter().collect();
            self.at = close + 1;
            let kind = match name.as_str() {
                "version" if seen.is_empty() => EventKind::VersionInformation,
                "encoding" if seen == [EventKind::VersionInformation] => EventKind::EncodingDeclaration,
                "standalone" if seen.first() == Some(&EventKind::VersionInformation) && !seen.contains(&EventKind::StandaloneDeclaration) && (value == "yes" || value == "no") => {
                    self.standalone_no = value == "no";
                    EventKind::StandaloneDeclaration
                }
                _ => return Err(Why::BadDeclaration),
            };
            seen.push(kind);
            self.pending.push_back(Event::new(kind, value));
        }
        if seen.first() != Some(&EventKind::VersionInformation) {
            return Err(Why::BadDeclaration);
        }
        self.at = end + 2;
        Ok(())
    }

    fn doctype(&mut self) -> Scan<()> {
        if self.place != Place::Prolog || self.doctype {
            return Err(Why::InvalidCharacter);
        }
        let mut depth = 0i32;
        let mut quote = None;
        let mut i = self.at + 9;
        let end = loop {
            let c = *self.buf.get(i).ok_or(MORE)?;
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '[') => depth += 1,
                (None, ']') => depth -= 1,
                (None, '>') if depth == 0 => break i,
                _ => {}
            }
            i += 1;
        };
        self.at += 9;
        if !self.spaces() {
            return Err(Why::InvalidCharacter);
        }
        let root = self.name()?;
        self.at = end + 1;
        self.doctype = true;
        self.pending.push_back(Event::new(EventKind::DocumentTypeDeclaration, root));
        Ok(())
    }

    fn instruction(&mut self) -> Scan<()> {
        self.at += 2;
        let start = self.at;
        let target = self.name()?;
        if target.eq_ignore_ascii_case("xml") {
            self.at = start;
            return Err(Why::BadDeclaration);
        }
        match self.peek(0) {
            None => return Err(MORE),
            Some('?') if self.peek(1).is_none() => return Err(MORE),
            Some('?') if self.peek(1) == Some('>') => {}
            Some(c) if is_space(c) => {}
            Some(_) => return Err(Why::InvalidCharacter),
        }
        self.spaces();
        self.open = Some(Open::Instruction(target.clone(), false));
        self.pending.push_back(Event::new(EventKind::ProcessingInstructionTarget, target));
        Ok(())
    }

    /// Text up to `terminator`, or, when the segment ends first, what it holds short of a possible
    /// start of the terminator.
    fn open_text(&mut self, terminator: &str) -> (String, bool) {
        match self.find(self.at, terminator) {
            Some(end) => {
                let text: String = self.buf[self.at..end].iter().collect();
                self.at = end + terminator.chars().count();
                (text, true)
            }
            None => {
                let rest = &self.buf[self.at..];
                let keep = (1..terminator.chars().count()).rev().find(|&n| n <= rest.len() && terminator.chars().take(n).eq(rest[rest.len() - n..].iter().copied())).unwrap_or(0);
                let end = self.buf.len() - keep;
                let text: String = self.buf[self.at..end].iter().collect();
                self.at = end;
                (text, false)
            }
        }
    }

    fn continue_open(&mut self, open: Open) -> Scan<()> {
        let (terminator, kind) = match &open {
            Open::Comment => ("-->", EventKind::Comment),
            Open::Instruction(..) => ("?>", EventKind::ProcessingInstructionData),
            Open::Cdata => ("]]>", EventKind::ContentCharacters),
        };
        let (text, complete) = self.open_text(terminator);
        if open == Open::Comment && text.contains("--") {
            return Err(Why::BadComment);
        }
        if !complete && text.is_empty() {
            return Err(MORE);
        }
        if let Open::Instruction(target, true) = &open {
            self.pending.push_back(Event::new(EventKind::ProcessingInstructionTarget, target.clone()));
        }
        if !text.is_empty() || kind == EventKind::Comment || kind == EventKind::ProcessingInstructionData {
            let mut event = Event::new(kind, text);
            if kind == EventKind::ContentCharacters {
                event.information = if complete { 1 } else { 2 };
            }
            self.pending.push_back(event);
        }
        if complete {
            self.open = None;
            if open == Open::Cdata {
                self.pending.push_back(Event::new(EventKind::EndOfCdataSection, ""));
            }
        } else if let Open::Instruction(target, _) = open {
            self.open = Some(Open::Instruction(target, true));
        }
        Ok(())
    }

    /// A run of character content, its references resolved, up to markup or the segment's end.
    fn content(&mut self) -> Scan<()> {
        let mut run = String::new();
        while let Some(c) = self.peek(0) {
            match c {
                '<' => break,
                '&' => {
                    let mark = self.at;
                    match self.reference() {
                        Ok(Ok(ch)) if (self.representable)(ch) || ch.is_ascii() => run.push(ch),
                        Ok(Ok(ch)) => {
                            if !run.is_empty() {
                                self.pending.push_back(Event::characters(EventKind::ContentCharacters, std::mem::take(&mut run), true));
                            }
                            self.pending.push_back(Event::new(EventKind::ContentNationalCharacter, ch.to_string()));
                        }
                        Ok(Err(name)) if self.doctype && self.standalone_no => {
                            if !run.is_empty() {
                                self.pending.push_back(Event::characters(EventKind::ContentCharacters, std::mem::take(&mut run), true));
                            }
                            self.pending.push_back(Event::new(EventKind::UnresolvedReference, name));
                        }
                        Ok(Err(_)) => {
                            self.at = mark;
                            return Err(Why::UndeclaredEntity);
                        }
                        Err(MORE) => {
                            self.at = mark;
                            break;
                        }
                        Err(why) => {
                            self.at = mark;
                            return Err(why);
                        }
                    }
                }
                '\r' => {
                    run.push('\n');
                    self.at += 1;
                    if self.peek(0) == Some('\n') {
                        self.at += 1;
                    }
                }
                c if !xml_char(c as u32) => return Err(Why::InvalidCharacter),
                c => {
                    run.push(c);
                    self.at += 1;
                }
            }
        }
        let complete = self.peek(0) == Some('<');
        if run.is_empty() && self.pending.is_empty() {
            return Err(MORE);
        }
        if !run.is_empty() {
            self.pending.push_back(Event::characters(EventKind::ContentCharacters, run, complete));
        }
        Ok(())
    }

    fn bound(&self, prefix: &str) -> Option<&str> {
        self.bindings.iter().rev().find(|(p, _)| p == prefix).map(|(_, uri)| uri.as_str())
    }

    fn start_tag(&mut self) -> Scan<()> {
        let tag_start = self.at;
        self.at += 1;
        let qualified = self.name()?;
        let mut attributes: Vec<(String, usize, String, Vec<Event>)> = Vec::new();
        let empty = loop {
            let spaced = self.spaces();
            match self.peek(0) {
                None => return Err(MORE),
                Some('>') => {
                    self.at += 1;
                    break false;
                }
                Some('/') => {
                    self.at += 1;
                    self.expect('>')?;
                    break true;
                }
                Some(_) if !spaced => return Err(Why::InvalidCharacter),
                Some(_) => {}
            }
            let name_at = self.at;
            let name = self.name()?;
            self.spaces();
            self.expect('=')?;
            self.spaces();
            let quote = match self.peek(0) {
                None => return Err(MORE),
                Some(q @ ('"' | '\'')) => q,
                Some(_) => return Err(Why::InvalidCharacter),
            };
            self.at += 1;
            let (value, parts) = self.attribute_value(quote)?;
            if attributes.iter().any(|(n, ..)| *n == name) {
                self.at = name_at;
                return Err(Why::DuplicateAttribute);
            }
            attributes.push((name, name_at, value, parts));
        };
        let scope = self.bindings.len();
        let mut declarations = Vec::new();
        for (name, _, value, _) in &attributes {
            let prefix = match name.as_str() {
                "xmlns" => Some(String::new()),
                n => n.strip_prefix("xmlns:").map(str::to_owned),
            };
            if let Some(prefix) = prefix {
                self.bindings.push((prefix.clone(), value.clone()));
                declarations.push(Event { namespace: value.clone(), prefix, ..Event::new(EventKind::NamespaceDeclaration, "") });
            }
        }
        let (prefix, local) = split(&qualified);
        let namespace = match self.bound(&prefix) {
            Some(uri) => uri.to_owned(),
            None if prefix.is_empty() => String::new(),
            None => {
                self.bindings.truncate(scope);
                self.at = tag_start + 1;
                return Err(Why::UndeclaredElementPrefix);
            }
        };
        let mut events = vec![Event { namespace: namespace.clone(), prefix: prefix.clone(), ..Event::new(EventKind::StartOfElement, local.clone()) }];
        events.extend(declarations);
        for (name, name_at, _, parts) in attributes {
            if name == "xmlns" || name.starts_with("xmlns:") {
                continue;
            }
            let (attribute_prefix, attribute_local) = split(&name);
            let attribute_namespace = if attribute_prefix.is_empty() {
                String::new()
            } else {
                match self.bound(&attribute_prefix) {
                    Some(uri) => uri.to_owned(),
                    None => {
                        self.bindings.truncate(scope);
                        self.at = name_at;
                        return Err(Why::UndeclaredAttributePrefix);
                    }
                }
            };
            events.push(Event { namespace: attribute_namespace, prefix: attribute_prefix, ..Event::new(EventKind::AttributeName, attribute_local) });
            events.extend(parts);
        }
        if self.place == Place::Epilogue {
            return Err(Why::ContentAfterRoot);
        }
        self.place = Place::Content;
        if empty {
            events.push(Event { namespace, prefix, ..Event::new(EventKind::EndOfElement, local) });
            self.bindings.truncate(scope);
            if self.elements.is_empty() {
                self.place = Place::Epilogue;
            }
        } else {
            self.elements.push(Element { qualified, local, prefix, namespace, bindings: scope });
        }
        self.pending.extend(events);
        Ok(())
    }

    /// An attribute value after its opening quote: the value, and its ATTRIBUTE-CHARACTERS and
    /// ATTRIBUTE-NATIONAL-CHARACTER events.
    fn attribute_value(&mut self, quote: char) -> Scan<(String, Vec<Event>)> {
        let (mut value, mut run, mut parts) = (String::new(), String::new(), Vec::new());
        loop {
            let Some(c) = self.peek(0) else { return Err(MORE) };
            match c {
                c if c == quote => {
                    self.at += 1;
                    break;
                }
                '<' => return Err(Why::LessThanInAttribute),
                '&' => match self.reference()? {
                    Ok(ch) if (self.representable)(ch) || ch.is_ascii() => {
                        run.push(ch);
                        value.push(ch);
                    }
                    Ok(ch) => {
                        if !run.is_empty() {
                            parts.push(Event::characters(EventKind::AttributeCharacters, std::mem::take(&mut run), true));
                        }
                        parts.push(Event::new(EventKind::AttributeNationalCharacter, ch.to_string()));
                        value.push(ch);
                    }
                    Err(_) => return Err(Why::UndeclaredEntity),
                },
                '\t' | '\n' | '\r' => {
                    run.push(' ');
                    value.push(' ');
                    self.at += 1;
                }
                c if !xml_char(c as u32) => return Err(Why::InvalidCharacter),
                c => {
                    run.push(c);
                    value.push(c);
                    self.at += 1;
                }
            }
        }
        if !run.is_empty() || parts.is_empty() {
            parts.push(Event::characters(EventKind::AttributeCharacters, run, true));
        }
        Ok((value, parts))
    }

    fn end_tag(&mut self) -> Scan<()> {
        let start = self.at;
        self.at += 2;
        let qualified = self.name()?;
        self.spaces();
        self.expect('>')?;
        match self.elements.last() {
            Some(e) if e.qualified == qualified => {}
            _ => {
                self.at = start;
                return Err(Why::MismatchedEndTag);
            }
        }
        let e = self.elements.pop().expect("checked above");
        self.bindings.truncate(e.bindings);
        self.pending.push_back(Event { namespace: e.namespace, prefix: e.prefix, ..Event::new(EventKind::EndOfElement, e.local) });
        if self.elements.is_empty() {
            self.place = Place::Epilogue;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
