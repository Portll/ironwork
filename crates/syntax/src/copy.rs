//! COPY and COPY REPLACING, applied to the logical text before it is lexed. Matching is by
//! text-words, as the standard defines them, and replacement edits the text itself, so a
//! pseudo-text such as `==:TAG:==` can replace part of a word like `:TAG:-RECORD`.

use crate::bms;
use crate::source::{self, Source};
use crate::system;
use crate::{Error, Pos};
use std::path::{Path, PathBuf};

/// Directories searched for COPY members, in order. A `COPY X OF LIB` looks in `<dir>/LIB` first.
/// The file being compiled, when named, is never one of its own members. The compliance level and
/// source format are how the program and its members are read.
#[derive(Clone, Debug, Default)]
pub struct Libraries {
    dirs: Vec<PathBuf>,
    program: Option<PathBuf>,
    compliance: numeric::Compliance,
    source_format: numeric::SourceFormat,
    relaxed: bool,
    loose: bool,
    empty_literal: numeric::EmptyLiteral,
    /// Members are read with cobc's tab stops, as a retry after a member did not read.
    member_tab_stops: bool,
    /// A source with no IDENTIFICATION DIVISION is a program named after its file, as a retry under
    /// `--compliance extended`.
    assume_program_id: bool,
    /// A source read for a function's definition does not look for its own functions' definitions.
    no_function_search: bool,
}

const COPYBOOKS: &[&str] = &[".cpy", ".CPY", ".copy", ".COPY"];
const PROGRAM_SOURCES: &[&str] = &[".cbl", ".CBL", ".cob", ".COB"];
const BARE: &[&str] = &[""];
const MAX_DEPTH: usize = 32;

impl Libraries {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs, program: None, compliance: numeric::Compliance::Strict, source_format: numeric::SourceFormat::Auto, relaxed: false, loose: false, empty_literal: numeric::EmptyLiteral::Space, member_tab_stops: false, assume_program_id: false, no_function_search: false }
    }

    /// These libraries, for compiling the program in `program`.
    pub fn with_program(&self, program: &Path) -> Self {
        Self { program: Some(program.to_path_buf()), ..self.clone() }
    }

    /// These libraries, read under `compliance`.
    pub fn with_compliance(&self, compliance: numeric::Compliance) -> Self {
        Self { compliance, ..self.clone() }
    }

    pub fn compliance(&self) -> numeric::Compliance {
        self.compliance
    }

    /// These libraries, read under the compliance level and source format `flags` give.
    pub fn with_flags(&self, flags: &[String]) -> Self {
        Self { compliance: numeric::Compliance::of(flags), source_format: numeric::SourceFormat::of(flags), relaxed: numeric::Compliance::relaxed(flags), loose: numeric::Compliance::loose(flags), empty_literal: numeric::EmptyLiteral::of(flags), ..self.clone() }
    }

    /// Under `--compliance relaxed`.
    pub fn relaxed(&self) -> bool {
        self.relaxed
    }

    /// Under `--compliance loose`.
    pub fn loose(&self) -> bool {
        self.loose
    }

    /// These libraries, a source with no IDENTIFICATION DIVISION read as a program named after its
    /// file.
    pub(crate) fn assuming_program_id(&self) -> Self {
        Self { assume_program_id: true, ..self.clone() }
    }

    /// The program file's name without its extension, which a program with no PROGRAM-ID takes
    /// under `--compliance extended`, on the retry that assumes one.
    pub fn program_stem(&self) -> Option<String> {
        let stem = self.program.as_deref()?.file_stem()?;
        (self.compliance == numeric::Compliance::Extended && self.assume_program_id).then(|| stem.to_string_lossy().into_owned())
    }

    /// These libraries, their members read with each tab reaching the next column after a multiple
    /// of 8.
    pub(crate) fn with_member_tab_stops(&self) -> Self {
        Self { member_tab_stops: true, ..self.clone() }
    }

    pub fn empty_literal(&self) -> numeric::EmptyLiteral {
        self.empty_literal
    }

    /// These libraries, read in `format` under `--compliance extended`.
    pub fn with_source_format(&self, source_format: numeric::SourceFormat) -> Self {
        Self { source_format, ..self.clone() }
    }

    pub fn source_format(&self) -> numeric::SourceFormat {
        self.source_format
    }

    /// A round of extensions searches every library before the next round starts, so a copybook in
    /// any library is found before a program source (assumptions C85 to C87).
    fn find(&self, name: &str, library: Option<&str>, literal: bool) -> Option<PathBuf> {
        let rounds = if literal { [BARE, COPYBOOKS, PROGRAM_SOURCES] } else { [COPYBOOKS, PROGRAM_SOURCES, BARE] };
        self.find_in_rounds(name, library, &rounds)
    }

    /// The definition or prototype of user-defined function `name` among the program sources of
    /// these libraries' directories, the program's own directory first and each directory's files in
    /// name order, read under these libraries without looking further: its prototype and its file.
    pub fn function_definition(&self, name: &str) -> Option<(crate::ast::Prototype, PathBuf)> {
        if self.no_function_search {
            return None;
        }
        let nested = Self { no_function_search: true, ..self.clone() };
        let wanted = name.to_ascii_uppercase();
        for d in &self.dirs {
            let Ok(entries) = std::fs::read_dir(d) else { continue };
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| PROGRAM_SOURCES.iter().any(|s| s[1..].eq_ignore_ascii_case(e))))
                .collect();
            files.sort();
            for path in files {
                if self.program.as_deref().is_some_and(|program| same_file(program, &path)) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else { continue };
                let text = decode(&bytes);
                let upper = text.to_ascii_uppercase();
                if !(upper.contains("FUNCTION-ID") && upper.contains(&wanted)) {
                    continue;
                }
                let Ok(programs) = crate::parse_all_with(&text, &nested.with_program(&path)) else { continue };
                if let Some(p) = programs.iter().flat_map(|p| &p.prototypes).find(|q| q.name.eq_ignore_ascii_case(&wanted)) {
                    return Some((p.clone(), path));
                }
            }
        }
        None
    }

    pub(crate) fn find_bms(&self, name: &str, library: Option<&str>) -> Option<PathBuf> {
        self.find_in_rounds(name, library, &[&[".bms", ".BMS"]])
    }

    fn find_in_rounds(&self, name: &str, library: Option<&str>, rounds: &[&[&str]]) -> Option<PathBuf> {
        let mut places: Vec<PathBuf> = Vec::new();
        for d in &self.dirs {
            if let Some(lib) = library {
                places.extend([lib.to_owned(), lib.to_ascii_lowercase()].iter().map(|l| d.join(l)));
            }
            places.push(d.clone());
        }
        let mut names = vec![name.to_owned()];
        for variant in [name.to_ascii_uppercase(), name.to_ascii_lowercase()] {
            if !names.contains(&variant) {
                names.push(variant);
            }
        }
        let (places, names) = (&places, &names);
        rounds
            .iter()
            .flat_map(|extensions| places.iter().flat_map(move |p| extensions.iter().flat_map(move |e| names.iter().map(move |n| p.join(format!("{n}{e}"))))))
            .find(|p| p.is_file() && !self.program.as_deref().is_some_and(|program| same_file(program, p)))
    }
}

/// Whether two paths name one file, however each is spelled.
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return x.dev() == y.dev() && x.ino() == y.ino();
        }
    }
    matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Reads a source file's bytes: UTF-8 when valid, otherwise one character per byte (Latin-1).
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Word {
    start: usize,
    end: usize,
    text: String,
}

fn text_words(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut i = 0;
    let separator_at = |i: usize| chars.get(i + 1).is_none_or(|c| c.is_whitespace());
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || ((c == ',' || c == ';') && separator_at(i)) {
            i += 1;
            continue;
        }
        let start = i;
        if c == '\'' || c == '"' {
            i += 1;
            while i < chars.len() {
                if chars[i] == c && chars.get(i + 1) == Some(&c) {
                    i += 2;
                } else if chars[i] == c {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if c == '=' && chars.get(i + 1) == Some(&'=') {
            i += 2;
        } else if matches!(c, '(' | ')' | ':') || (c == '.' && separator_at(i)) {
            i += 1;
        } else {
            while i < chars.len() {
                let d = chars[i];
                let ends = d.is_whitespace()
                    || matches!(d, '(' | ')' | ':' | '\'' | '"')
                    || ((d == '.' || d == ',' || d == ';') && separator_at(i))
                    || (d == '=' && chars.get(i + 1) == Some(&'='));
                if ends {
                    break;
                }
                i += 1;
            }
        }
        words.push(Word { start, end: i, text: chars[start..i].iter().collect() });
    }
    words
}

fn same(a: &str, b: &str) -> bool {
    if a.starts_with(['\'', '"']) { a == b } else { a.eq_ignore_ascii_case(b) }
}

#[derive(Clone, Debug)]
enum Mode {
    Whole,
    Leading,
    Trailing,
}

#[derive(Clone, Debug)]
struct Replacing {
    mode: Mode,
    pattern: Vec<String>,
    replacement: String,
}

struct Statement {
    name: String,
    /// Whether the name is a literal, which IBM takes as a file name as written.
    literal: bool,
    library: Option<String>,
    replacing: Vec<Replacing>,
    /// Index of the word after the terminating period.
    next: usize,
    /// The warning for a name read without the periods after it, under `--compliance extended`.
    note: Option<Error>,
}

fn copy_statement(words: &[Word], chars: &[char], at: usize, pos: Pos, extended: bool) -> Result<Statement, Error> {
    let err = |m: &str| crate::messages::IWS0003.at(pos, format!("COPY: {m}"));
    let text = |i: usize| words.get(i).map(|w| w.text.as_str());
    let mut i = at + 1;
    let quoted = |s: &str| s.starts_with(['\'', '"']);
    let unquote = |s: &str| s.trim_matches(|c| c == '\'' || c == '"').to_owned();
    // In `COPY X..` the separator period is the second, so the name is `X.` (assumption C88).
    let word = |s: &str| {
        if s.ends_with('.') && !quoted(s) {
            return Err(crate::messages::IWS0004.at(pos, format!("COPY {s}: the name ends in a period; the period that ends a COPY statement is the one followed by a space")));
        }
        Ok(unquote(s))
    };
    let first = text(i).ok_or_else(|| err("a member name"))?;
    let mut note = None;
    let first = match first.trim_end_matches('.') {
        bare if extended && bare.len() < first.len() && !bare.is_empty() && !quoted(first) && text(i + 1).is_none_or(|w| !w.eq_ignore_ascii_case("OF") && !w.eq_ignore_ascii_case("IN")) => {
            note = Some(crate::messages::IWX0054.at(pos, format!("COPY {first}. (GnuCOBOL and Micro Focus; Enterprise COBOL reads the name as {first}): the member is {bare}, and the periods after it end the statement")));
            bare
        }
        _ => first,
    };
    let (name, literal) = (word(first)?, quoted(first));
    i += 1;
    let mut library = None;
    if text(i).is_some_and(|w| w.eq_ignore_ascii_case("OF") || w.eq_ignore_ascii_case("IN")) {
        library = Some(word(text(i + 1).ok_or_else(|| err("a library name"))?)?);
        i += 2;
    }
    if text(i).is_some_and(|w| w.eq_ignore_ascii_case("SUPPRESS")) {
        i += 1;
    }
    let mut replacing = Vec::new();
    if text(i).is_some_and(|w| w.eq_ignore_ascii_case("REPLACING")) {
        (replacing, i) = operands(words, chars, i + 1, "COPY", pos)?;
    }
    if text(i) != Some(".") {
        return Err(err("a period to end the statement"));
    }
    Ok(Statement { name, literal, library, replacing, next: i + 1, note })
}

/// The operand pairs of COPY REPLACING or of REPLACE, from word `at` to the period that ends the
/// statement, and that period's index. REPLACE takes pseudo-text alone (Language Reference
/// SC27-8713-03, p. 708).
fn operands(words: &[Word], chars: &[char], at: usize, verb: &str, pos: Pos) -> Result<(Vec<Replacing>, usize), Error> {
    let err = |m: &str| crate::messages::IWS0005.at(pos, format!("{verb}: {m}"));
    let text = |i: usize| words.get(i).map(|w| w.text.as_str());
    let replace = verb == "REPLACE";
    let pseudo_text = |i: usize| !replace || text(i) == Some("==");
    let (first, second) = if replace { ("pseudo-text between == delimiters to replace", "pseudo-text between == delimiters after BY") } else { ("an operand to replace", "an operand after BY") };
    let (mut replacing, mut i) = (Vec::new(), at);
    while text(i).is_some_and(|w| w != ".") {
        let mode = match text(i) {
            Some(w) if w.eq_ignore_ascii_case("LEADING") => Mode::Leading,
            Some(w) if w.eq_ignore_ascii_case("TRAILING") => Mode::Trailing,
            _ => Mode::Whole,
        };
        if !matches!(mode, Mode::Whole) {
            i += 1;
        }
        let (pattern, _, after) = operand(words, chars, i).filter(|_| pseudo_text(i)).ok_or_else(|| err(first))?;
        if !text(after).is_some_and(|w| w.eq_ignore_ascii_case("BY")) {
            return Err(err("BY"));
        }
        let (_, replacement, after) = operand(words, chars, after + 1).filter(|_| pseudo_text(after + 1)).ok_or_else(|| err(second))?;
        if pattern.is_empty() || (!matches!(mode, Mode::Whole) && pattern.len() != 1) {
            return Err(err("LEADING and TRAILING take one word; an empty pattern matches nothing"));
        }
        replacing.push(Replacing { mode, pattern, replacement });
        i = after;
    }
    Ok((replacing, i))
}

/// A REPLACING operand at word `at`: pseudo-text `==...==`, or an identifier, literal, word or
/// function-identifier, which matches as pseudo-text holding it (Language Reference SC27-8713-03,
/// p. 690). Returns its text-words, its text as written, which is what it copies in, and the
/// index of the word after it.
fn operand(words: &[Word], chars: &[char], at: usize) -> Option<(Vec<String>, String, usize)> {
    let pseudo_text = words.get(at)?.text == "==";
    let (from, to) = if pseudo_text { (at + 1, words[at + 1..].iter().position(|w| w.text == "==")? + at + 1) } else { (at, operand_end(words, at)) };
    let text = if from < to { chars[words[from].start..words[to - 1].end].iter().collect() } else { String::new() };
    Some((words[from..to].iter().map(|w| w.text.clone()).collect(), text, to + usize::from(pseudo_text)))
}

/// The index of the word after the operand that starts at word `at`: a literal is one word, ALL
/// takes the literal after it, FUNCTION the function's name, and a name its IN or OF qualifiers,
/// then its subscripts and reference modification in parentheses.
fn operand_end(words: &[Word], at: usize) -> usize {
    let is = |i: usize, w: &str| words.get(i).is_some_and(|x| x.text.eq_ignore_ascii_case(w));
    if words[at].text.starts_with(['\'', '"']) {
        return at + 1;
    }
    if is(at, "ALL") {
        return (at + 2).min(words.len());
    }
    let mut i = at + 1 + usize::from(is(at, "FUNCTION") && at + 1 < words.len());
    while (is(i, "IN") || is(i, "OF")) && i + 1 < words.len() {
        i += 2;
    }
    while is(i, "(") {
        let mut depth = 0;
        while let Some(w) = words.get(i) {
            depth += i32::from(w.text == "(") - i32::from(w.text == ")");
            i += 1;
            if depth == 0 {
                break;
            }
        }
    }
    i
}

/// Appends `chars[range]` to `out` with the positions they came from.
fn copy_span(out: &mut Source, chars: &[char], positions: &[Pos], range: std::ops::Range<usize>) {
    out.text.extend(&chars[range.clone()]);
    out.positions.extend_from_slice(&positions[range]);
}

fn apply(src: &Source, replacing: &[Replacing]) -> Source {
    if replacing.is_empty() {
        return Source { text: src.text.clone(), positions: src.positions.clone(), options: Vec::new(), debugging: None, free: Vec::new(), notes: Vec::new(), constants: Vec::new() };
    }
    let chars: Vec<char> = src.text.chars().collect();
    let words = text_words(&chars);
    let mut out = Source { text: String::new(), positions: Vec::new(), options: Vec::new(), debugging: None, free: Vec::new(), notes: Vec::new(), constants: Vec::new() };
    let emit = |out: &mut Source, text: &str, pos: Pos| {
        for c in text.chars() {
            out.text.push(c);
            out.positions.push(pos);
        }
    };
    let (mut cursor, mut i) = (0usize, 0usize);
    while i < words.len() {
        copy_span(&mut out, &chars, &src.positions, cursor..words[i].start);
        let pos = src.positions[words[i].start];
        let hit = replacing.iter().find_map(|r| match r.mode {
            Mode::Whole => (words.len() - i >= r.pattern.len() && r.pattern.iter().zip(&words[i..]).all(|(p, w)| same(p, &w.text)))
                .then(|| (r.replacement.clone(), r.pattern.len())),
            Mode::Leading => {
                let w = &words[i].text;
                (w.len() > r.pattern[0].len() || w.eq_ignore_ascii_case(&r.pattern[0]))
                    .then_some(())
                    .filter(|_| w.to_ascii_uppercase().starts_with(&r.pattern[0].to_ascii_uppercase()))
                    .map(|_| (format!("{}{}", r.replacement, &w[r.pattern[0].len()..]), 1))
            }
            Mode::Trailing => {
                let w = &words[i].text;
                w.to_ascii_uppercase()
                    .ends_with(&r.pattern[0].to_ascii_uppercase())
                    .then(|| (format!("{}{}", &w[..w.len() - r.pattern[0].len()], r.replacement), 1))
            }
        });
        match hit {
            Some((text, consumed)) => {
                emit(&mut out, &text, pos);
                cursor = words[i + consumed - 1].end;
                i += consumed;
            }
            None => {
                copy_span(&mut out, &chars, &src.positions, words[i].start..words[i].end);
                cursor = words[i].end;
                i += 1;
            }
        }
    }
    copy_span(&mut out, &chars, &src.positions, cursor..chars.len());
    out
}

/// Replaces every COPY statement in `source` with its member's text, recursively. `files` names
/// each source file; a position's `file` indexes it.
pub fn expand(source: Source, libraries: &Libraries, files: &mut Vec<String>) -> Result<Source, Error> {
    let mut stack = Vec::new();
    expand_nested(source, libraries, files, &mut stack)
}

/// `EXEC SQL INCLUDE name END-EXEC`, which the Db2 precompiler treats as a COPY of its member (and
/// of its own SQLCA or SQLDA): the member's name, whether it is quoted, and the word after END-EXEC
/// and any period that ends it.
fn sql_include(words: &[Word], at: usize) -> Option<(String, bool, usize)> {
    let is = |k: usize, w: &str| words.get(at + k).is_some_and(|x| x.text.eq_ignore_ascii_case(w));
    if !(is(0, "EXEC") && is(1, "SQL") && is(2, "INCLUDE") && is(4, "END-EXEC")) {
        return None;
    }
    let word = &words[at + 3].text;
    let name = word.trim_matches(|c| c == '\'' || c == '"').to_owned();
    Some((name, word.starts_with(['\'', '"']), at + 5 + usize::from(is(5, "."))))
}

fn expand_nested(source: Source, libraries: &Libraries, files: &mut Vec<String>, stack: &mut Vec<String>) -> Result<Source, Error> {
    let chars: Vec<char> = source.text.chars().collect();
    let words = text_words(&chars);
    if !words.iter().any(|w| w.text.eq_ignore_ascii_case("COPY") || w.text.eq_ignore_ascii_case("INCLUDE")) {
        return Ok(source);
    }
    let mut out = Source { text: String::new(), positions: Vec::new(), options: source.options.clone(), debugging: source.debugging.clone(), free: source.free.clone(), notes: source.notes.clone(), constants: source.constants.clone() };
    let read = |text: &str, file: u16| source::read_under(text, file, source.debugging.is_some(), libraries.compliance());
    let (mut cursor, mut i) = (0usize, 0usize);
    while i < words.len() {
        let pos = source.positions[words[i].start];
        let (name, literal, library, replacing, next, sql) = if words[i].text.eq_ignore_ascii_case("COPY") {
            let st = copy_statement(&words, &chars, i, pos, libraries.compliance() == numeric::Compliance::Extended)?;
            out.notes.extend(st.note);
            (st.name, st.literal, st.library, st.replacing, st.next, false)
        } else if let Some((name, literal, next)) = sql_include(&words, i) {
            (name, literal, None, Vec::new(), next, true)
        } else {
            i += 1;
            continue;
        };
        copy_span(&mut out, &chars, &source.positions, cursor..words[i].start);
        let own = sql && matches!(name.to_ascii_uppercase().as_str(), "SQLCA" | "SQLDA");
        let path = if own { None } else { libraries.find(&name, library.as_deref(), literal) };
        let verb = if sql { "EXEC SQL INCLUDE" } else { "COPY" };
        let mapset = if own || path.is_some() { None } else { bms::load(libraries, &name, library.as_deref()) };
        let (key, member) = match (path, mapset) {
            (Some(path), _) => {
                let copied_free = source.free_at(pos).is_some();
                let detect = libraries.source_format() != numeric::SourceFormat::Fixed;
                let copied = |text: &str, file: u16| source::read_copied(text, file, source.debugging.is_some(), libraries.compliance(), copied_free, detect, libraries.member_tab_stops);
                (path.display().to_string(), read_member(&path, pos, files, &copied)?)
            }
            (None, Some((path, mapset))) => {
                let mapset = mapset.map_err(|e| crate::messages::IWS0006.at(pos, format!("{verb} {name}: {}", e.place(&path.display().to_string()))))?;
                let file = u16::try_from(files.len()).map_err(|_| crate::messages::IWS0007.at(pos, "more than 65535 copy members"))?;
                files.push(path.display().to_string());
                (path.display().to_string(), read(&bms::symbolic_map(&mapset), file)?)
            }
            (None, None) => {
                let text = system::member(&name).ok_or_else(|| crate::messages::IWS0002.at(pos, format!("{verb} {name}: no such member in the copy libraries")))?;
                let key = format!("(system member {})", name.to_ascii_uppercase());
                let file = u16::try_from(files.len()).map_err(|_| crate::messages::IWS0007.at(pos, "more than 65535 copy members"))?;
                files.push(key.clone());
                (key, read(&text, file)?)
            }
        };
        if stack.contains(&key) || stack.len() >= MAX_DEPTH {
            return Err(crate::messages::IWS0008.at(pos, format!("{verb} {name}: copies itself, or nests deeper than {MAX_DEPTH}")));
        }
        stack.push(key);
        let mut member = expand_nested(member, libraries, files, stack)?;
        stack.pop();
        if let (Some(lines), Some(copied)) = (&mut out.debugging, member.debugging.take()) {
            // A COPY on a debugging line makes all of its member's text debugging lines.
            if lines.contains(&(pos.file, pos.line)) {
                lines.extend(member.positions.iter().map(|p| (p.file, p.line)));
            }
            lines.extend(copied);
        }
        out.free.extend(member.free.iter().cloned());
        out.notes.extend(member.notes.iter().cloned());
        out.constants.extend(member.constants.iter().cloned());
        let replaced = apply(&member, &replacing);
        out.text.push_str(&replaced.text);
        out.positions.extend(replaced.positions);
        out.text.push('\n');
        out.positions.push(pos);
        cursor = words.get(next - 1).map_or(chars.len(), |w| w.end);
        i = next;
    }
    copy_span(&mut out, &chars, &source.positions, cursor..chars.len());
    Ok(out)
}

/// Applies the REPLACE statements in `source`, which COPY has expanded (Language Reference
/// SC27-8713-03, pp. 708-712): each one's operands act on the text from its period to the next
/// REPLACE statement or the end of the source, with COPY REPLACING's matching, and the statements
/// themselves are left out (assumption C160).
pub fn replace(source: Source) -> Result<Source, Error> {
    let chars: Vec<char> = source.text.chars().collect();
    let words = text_words(&chars);
    let starts = |i: usize| words[i].text.eq_ignore_ascii_case("REPLACE");
    if !(0..words.len()).any(starts) {
        return Ok(source);
    }
    let mut out = Source { text: String::new(), positions: Vec::new(), options: source.options.clone(), debugging: source.debugging.clone(), free: source.free.clone(), notes: source.notes.clone(), constants: source.constants.clone() };
    let segment = |out: &mut Source, range: std::ops::Range<usize>, active: &[Replacing]| {
        let text = Source { text: chars[range.clone()].iter().collect(), positions: source.positions[range].to_vec(), options: Vec::new(), debugging: None, free: Vec::new(), notes: Vec::new(), constants: Vec::new() };
        let replaced = apply(&text, active);
        out.text.push_str(&replaced.text);
        out.positions.extend(replaced.positions);
    };
    let (mut active, mut cursor, mut i) = (Vec::new(), 0usize, 0usize);
    while i < words.len() {
        if !starts(i) {
            i += 1;
            continue;
        }
        let pos = source.positions[words[i].start];
        let next = match words.get(i + 1).map(|w| w.text.to_ascii_uppercase()).as_deref() {
            Some("OFF") if words.get(i + 2).is_some_and(|w| w.text == ".") => (Vec::new(), i + 3),
            Some("OFF") => return Err(crate::messages::IWS0009.at(pos, "REPLACE OFF: a period to end the statement")),
            Some("ALSO" | "LAST") => return Err(crate::messages::IWS0010.at(pos, "REPLACE ALSO and REPLACE LAST OFF are the 2014 COBOL standard's; Enterprise COBOL has REPLACE pseudo-text BY pseudo-text and REPLACE OFF")),
            Some("==" | "LEADING" | "TRAILING") => {
                let (replacing, period) = operands(&words, &chars, i + 1, "REPLACE", pos)?;
                if words.get(period).is_none_or(|w| w.text != ".") {
                    return Err(crate::messages::IWS0011.at(pos, "REPLACE: a period to end the statement"));
                }
                (replacing, period + 1)
            }
            _ => {
                i += 1;
                continue;
            }
        };
        segment(&mut out, cursor..words[i].start, &active);
        (active, i) = next;
        cursor = words[i - 1].end;
    }
    segment(&mut out, cursor..chars.len(), &active);
    Ok(out)
}

fn read_member(path: &Path, pos: Pos, files: &mut Vec<String>, read: &dyn Fn(&str, u16) -> Result<Source, Error>) -> Result<Source, Error> {
    let bytes = std::fs::read(path).map_err(|e| crate::messages::IWS0012.at(pos, format!("COPY {}: {e}", path.display())))?;
    let file = u16::try_from(files.len()).map_err(|_| crate::messages::IWS0007.at(pos, "more than 65535 copy members"))?;
    files.push(path.display().to_string());
    read(&decode(&bytes), file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        text_words(&s.chars().collect::<Vec<_>>()).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn text_words_follow_the_separators() {
        assert_eq!(words("01 :TAG:-REC PIC X(3)."), ["01", ":", "TAG", ":", "-REC", "PIC", "X", "(", "3", ")", "."]);
        assert_eq!(words("MOVE 'A B' TO X, Y."), ["MOVE", "'A B'", "TO", "X", "Y", "."]);
        assert_eq!(words("==A== BY ==B==."), ["==", "A", "==", "BY", "==", "B", "==", "."]);
        assert_eq!(words("GENAUW.CLAIM 0.1"), ["GENAUW.CLAIM", "0.1"]);
    }

    fn dir_with(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ironwork-copy-{}-{}", std::process::id(), files[0].0));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        dir
    }

    fn expanded(main: &str, dir: &Path) -> Result<String, Error> {
        let src = source::read(main)?;
        let mut files = vec![String::new()];
        expand(src, &Libraries::new(vec![dir.to_path_buf()]), &mut files).map(|s| s.text)
    }

    #[test]
    fn copy_replacing_whole_words_and_tags_inside_words() {
        let dir = dir_with(&[("RECS.cpy", "       01  :TAG:-REC.\n           05 :TAG:-ID PIC 9(4) VALUE OLD.\n")]);
        let text = expanded("           COPY RECS REPLACING ==:TAG:== BY ==CUST== OLD BY 42.\n", &dir).unwrap();
        assert!(text.contains("01  CUST-REC."), "{text}");
        assert!(text.contains("05 CUST-ID PIC 9(4) VALUE 42."), "{text}");
        assert!(!text.contains("COPY"));
    }

    #[test]
    fn leading_and_trailing_replace_part_of_a_word() {
        let dir = dir_with(&[("PART.cpy", "       01  WS-A PIC X.\n       01  B-WS PIC X.\n")]);
        let text = expanded("           COPY PART REPLACING LEADING ==WS== BY ==LK==\n               TRAILING ==WS== BY ==XX==.\n", &dir).unwrap();
        assert!(text.contains("LK-A") && text.contains("B-XX"), "{text}");
    }

    #[test]
    fn identifier_operands_take_their_qualifiers_subscripts_and_reference_modification() {
        let dir = dir_with(&[("IDOPS.cpy", "           MOVE OLD-1 TO OLD-2.\n           ADD 1 TO A IN B (1).\n           DISPLAY FUNCTION UPPER-CASE (X).\n")]);
        let text = expanded(
            concat!(
                "           COPY IDOPS REPLACING OLD-1 BY NEW-Q OF NEW-R\n",
                "                                    IN NEW-S\n",
                "                OLD-2 BY Z (2, 1, 1) (1:3)\n",
                "                A IN B (1) BY C\n",
                "                FUNCTION UPPER-CASE (X) BY Y.\n",
            ),
            &dir,
        )
        .unwrap();
        let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(words, "MOVE NEW-Q OF NEW-R IN NEW-S TO Z (2, 1, 1) (1:3). ADD 1 TO C. DISPLAY Y.");
    }

    #[test]
    fn pseudo_text_is_copied_as_written_and_a_member_may_end_the_entry_that_copies_it() {
        let dir = dir_with(&[("K101.cpy", "             .\n           02 TST-FLD-1 PICTURE 9(5).\n           02 FILLER    PICTURE X(115).\n")]);
        let text = expanded("       01  TST-TEST COPY K101 REPLACING TST-FLD-1 BY TF-1.\n", &dir).unwrap();
        assert!(text.contains("01  TST-TEST") && text.contains("02 TF-1 PICTURE 9(5)."), "{text}");
        let text = expanded(
            concat!(
                "       01  TEXT-TEST-1 COPY K101\n",
                "           REPLACING ==02 TST-FLD-1  PICTURE 9(5). 02 FILLER\n",
                "                       PICTURE X(115)==\n",
                "           BY        ==02 FILLER PICTURE X(115).  02 TXT-FLD-1\n",
                "                       PIC 9(5)==.\n",
            ),
            &dir,
        )
        .unwrap();
        let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(words, "01 TEXT-TEST-1 . 02 FILLER PICTURE X(115). 02 TXT-FLD-1 PIC 9(5).");
    }

    #[test]
    fn nested_copies_and_a_missing_member() {
        let dir = dir_with(&[("OUTER.cpy", "       COPY INNER.\n"), ("inner.cpy", "       01  X PIC X.\n")]);
        assert!(expanded("       COPY OUTER.\n", &dir).unwrap().contains("01  X PIC X."));
        let err = expanded("       COPY NOPE.\n", &dir).unwrap_err();
        assert!(err.message.contains("NOPE"));
    }

    #[test]
    fn system_members_answer_when_no_library_does() {
        let dir = dir_with(&[("OTHER.cpy", "       01  O PIC X.\n")]);
        let text = expanded("       COPY DFHAID.\n           EXEC SQL INCLUDE SQLCA END-EXEC.\n", &dir).unwrap();
        assert!(text.contains("DFHENTER") && text.contains("SQLCODE"), "{text}");
        assert!(!text.contains("END-EXEC"));
        let dir = dir_with(&[("DFHAID.cpy", "       01  VENDORED PIC X.\n")]);
        assert!(expanded("       COPY DFHAID.\n", &dir).unwrap().contains("VENDORED"));
    }

    #[test]
    fn a_member_that_copies_itself_is_refused() {
        let dir = dir_with(&[("LOOP.cpy", "       COPY LOOP.\n")]);
        assert!(expanded("       COPY LOOP.\n", &dir).unwrap_err().message.contains("copies itself"));
    }

    /// A fresh directory holding `files`, each at its path under it.
    fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ironwork-copy-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (name, text) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    fn expanded_with(main: &str, libraries: &Libraries) -> Result<String, Error> {
        let mut files = vec![String::new()];
        expand(source::read(main)?, libraries, &mut files).map(|s| s.text)
    }

    #[test]
    fn a_copybook_in_any_library_is_found_before_a_program_source() {
        let root = tree(
            "rounds",
            &[
                ("src/INQACC.cbl", "       01  PROGRAM-SOURCE PIC X.\n"),
                ("src/ONLY.cbl", "       01  ONLY-SOURCE PIC X.\n"),
                ("src/BARE", "       01  BARE-FILE PIC X.\n"),
                ("cpy/INQACC.cpy", "       01  COPYBOOK PIC X.\n"),
                ("cpy/BARE.copy", "       01  COPY-FILE PIC X.\n"),
            ],
        );
        let libraries = Libraries::new(vec![root.join("src"), root.join("cpy")]);
        assert!(expanded_with("       COPY INQACC.\n", &libraries).unwrap().contains("COPYBOOK"));
        assert!(expanded_with("       COPY inqacc.\n", &libraries).unwrap().contains("COPYBOOK"));
        assert!(expanded_with("       COPY BARE.\n", &libraries).unwrap().contains("COPY-FILE"));
        assert!(expanded_with("       COPY ONLY.\n", &libraries).unwrap().contains("ONLY-SOURCE"));
    }

    #[test]
    fn a_name_alone_is_tried_after_its_extensions_and_a_literal_first() {
        let root = tree("bare", &[("lib/MEMBER", "       01  BARE-FILE PIC X.\n"), ("lib/MEMBER.cpy", "       01  COPYBOOK PIC X.\n"), ("lib/ALONE", "       01  ALONE PIC X.\n")]);
        let libraries = Libraries::new(vec![root.join("lib")]);
        assert!(expanded_with("       COPY MEMBER.\n", &libraries).unwrap().contains("COPYBOOK"));
        assert!(expanded_with("       COPY \"MEMBER\".\n", &libraries).unwrap().contains("BARE-FILE"));
        assert!(expanded_with("       COPY ALONE.\n", &libraries).unwrap().contains("01  ALONE"));
    }

    #[test]
    fn the_program_being_compiled_is_never_its_own_member() {
        let program = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. PGMC.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       COPY PGMC.\n";
        let root = tree("self", &[("src/PGMC.cbl", program), ("src/OUTER.cpy", "       COPY PGMC.\n"), ("lib/PGMC.cbl", "       01  PGMC-X PIC X.\n")]);
        let main = root.join("src/../src/PGMC.cbl");
        let libraries = Libraries::new(vec![root.join("src"), root.join("lib")]).with_program(&main);
        assert!(expanded_with(program, &libraries).unwrap().contains("PGMC-X"));
        assert!(expanded_with("       COPY OUTER.\n", &libraries).unwrap().contains("PGMC-X"));
        let alone = Libraries::new(vec![root.join("src")]).with_program(&main);
        assert!(expanded_with(program, &alone).unwrap_err().message.contains("no such member"));
        assert!(expanded_with(program, &Libraries::new(vec![root.join("src")])).unwrap_err().message.contains("copies itself"));
    }

    #[test]
    fn a_doubled_period_is_refused_by_name() {
        let root = tree("period", &[("lib/COBCPARMS.cpy", "       01  PARMS PIC X.\n")]);
        let libraries = Libraries::new(vec![root.join("lib")]);
        let err = expanded_with("       COPY COBCPARMS..\n", &libraries).unwrap_err();
        assert!(err.message.contains("COPY COBCPARMS.: the name ends in a period"), "{}", err.message);
        let err = expanded_with("       COPY COBCPARMS OF LIB..\n", &libraries).unwrap_err();
        assert!(err.message.contains("COPY LIB.: "), "{}", err.message);
        assert!(expanded_with("       COPY COBCPARMS.\n", &libraries).unwrap().contains("PARMS"));
    }

    fn replaced(text: &str) -> Result<String, Error> {
        replace(source::read(text)?).map(|s| s.text)
    }

    #[test]
    fn replace_acts_from_its_period_to_the_next_replace() {
        let text = replaced(concat!(
            "       01  A PICTURE X.\n",
            "       REPLACE ==PICTURE== BY ==PIC==.\n",
            "       01  B PICTURE X.\n",
            "       01  C PICTURE X VALUE 'PICTURE'.\n",
            "       REPLACE OFF.\n",
            "       01  D PICTURE X.\n",
        ))
        .unwrap();
        let words: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(words, ["01", "A", "PICTURE", "X.", "01", "B", "PIC", "X.", "01", "C", "PIC", "X", "VALUE", "'PICTURE'.", "01", "D", "PICTURE", "X."]);
    }

    #[test]
    fn replace_takes_the_language_references_example() {
        let text = replaced(concat!(
            "           REPLACE ==\"(Hello, World!)\"== BY ==\"(Hello, Mom!)\"==.\n",
            "       01 WS-STRING1 PIC X(30) VALUE \"(Hello, World!)\".\n",
            "           DISPLAY \"Modified: \" XX-WS-:TAG:1\n",
            "           REPLACE LEADING ==XX-==  BY ====\n",
            "                           ==:TAG:==  BY ==STRING==\n",
            "                   TRAILING ==1== BY ==2==.\n",
            "           DISPLAY \"Modified: \" XX-WS-:TAG:1\n",
        ))
        .unwrap();
        assert!(text.contains("VALUE \"(Hello, Mom!)\"."), "{text}");
        assert!(text.contains("DISPLAY \"Modified: \" XX-WS-:TAG:1\n") && text.ends_with("DISPLAY \"Modified: \" WS-STRING2"), "{text}");
        assert!(!text.contains("REPLACE"), "{text}");
    }

    #[test]
    fn a_later_replace_supersedes_and_several_words_match_as_one() {
        let text = replaced(concat!(
            "           REPLACE ==AO== BY ==TO== == = == BY ==EQUAL==.\n",
            "           MOVE \"*\" AO X.\n",
            "           REPLACE ==MOVE \"*\" TO X.\n",
            "                      IF X = \"*\"== BY ==DISPLAY X==.\n",
            "           MOVE \"*\" TO X.\n",
            "           IF X = \"*\" DISPLAY Y.\n",
        ))
        .unwrap();
        assert!(text.contains("MOVE \"*\" TO X."), "{text}");
        assert!(text.contains("DISPLAY X DISPLAY Y."), "{text}");
    }

    #[test]
    fn replace_takes_only_pseudo_text() {
        let text = replaced("           MOVE REPLACE TO X.\n           DISPLAY Y.\n").unwrap();
        assert!(text.contains("MOVE REPLACE TO X."));
        assert!(replaced("       REPLACE A BY B.\n").unwrap().contains("REPLACE A BY B."));
        let err = replaced("       REPLACE ==A== BY B.\n").unwrap_err();
        assert_eq!(err.message, "REPLACE: pseudo-text between == delimiters after BY");
        let err = replaced("       REPLACE ALSO ==A== BY ==B==.\n").unwrap_err();
        assert!(err.message.starts_with("REPLACE ALSO and REPLACE LAST OFF are the 2014"), "{}", err.message);
        assert_eq!(replaced("       REPLACE ==A== BY ==B==\n").unwrap_err().message, "REPLACE: a period to end the statement");
    }

    #[test]
    fn replace_acts_on_copied_text() {
        let dir = dir_with(&[("REPMEM.cpy", "       01  :TAG:-ID PIC 9.\n")]);
        let src = source::read("       REPLACE ==:TAG:== BY ==CUST==.\n       COPY REPMEM.\n").unwrap();
        let mut files = vec![String::new()];
        let text = expand(src, &Libraries::new(vec![dir]), &mut files).and_then(replace).unwrap().text;
        assert!(text.contains("01  CUST-ID PIC 9."), "{text}");
    }

    #[test]
    fn positions_in_a_member_name_its_file() {
        let dir = dir_with(&[("POS.cpy", "       01  Y PIC X.\n")]);
        let src = source::read("       COPY POS.\n").unwrap();
        let mut files = vec![String::new()];
        let out = expand(src, &Libraries::new(vec![dir]), &mut files).unwrap();
        let at = out.text.find('Y').unwrap();
        assert_eq!(out.positions[at].file, 1);
        assert!(files[1].ends_with("POS.cpy"));
    }

    #[test]
    fn under_extended_a_name_followed_by_two_periods_names_the_member_without_them() {
        let dir = std::env::temp_dir().join(format!("iw-copy-dots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CSTMT.cpy"), "           DISPLAY 'FROM COPY'.\n").unwrap();
        let main = "       PROCEDURE DIVISION.\n           COPY CSTMT..\n";
        let libraries = Libraries::new(vec![dir.clone()]).with_compliance(numeric::Compliance::Extended);
        let source = crate::source::read_under(main, 0, false, numeric::Compliance::Extended).unwrap();
        let expanded = expand(source, &libraries, &mut Vec::new()).unwrap();
        assert!(expanded.text.contains("DISPLAY 'FROM COPY'"), "{}", expanded.text);
        assert_eq!(expanded.notes.iter().map(|n| (n.pos.line, n.id)).collect::<Vec<_>>(), [(2, Some("IWX0054"))]);
        let strict = expand(crate::source::read_under(main, 0, false, numeric::Compliance::Strict).unwrap(), &Libraries::new(vec![dir.clone()]), &mut Vec::new());
        assert_eq!(strict.err().map(|e| e.id), Some(Some("IWS0004")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
