//! COPY and COPY REPLACING, applied to the logical text before it is lexed. Matching is by
//! text-words, as the standard defines them, and replacement edits the text itself, so a
//! pseudo-text such as `==:TAG:==` can replace part of a word like `:TAG:-RECORD`.

use crate::bms;
use crate::source::{self, Source};
use crate::system;
use crate::{Error, Pos};
use std::path::{Path, PathBuf};

/// Directories searched for COPY members, in order. A `COPY X OF LIB` looks in `<dir>/LIB` first.
/// The file being compiled, when named, is never one of its own members.
#[derive(Clone, Debug, Default)]
pub struct Libraries {
    dirs: Vec<PathBuf>,
    program: Option<PathBuf>,
}

const COPYBOOKS: &[&str] = &[".cpy", ".CPY", ".copy", ".COPY"];
const PROGRAM_SOURCES: &[&str] = &[".cbl", ".CBL", ".cob", ".COB"];
const BARE: &[&str] = &[""];
const MAX_DEPTH: usize = 32;

impl Libraries {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs, program: None }
    }

    /// These libraries, for compiling the program in `program`.
    pub fn with_program(&self, program: &Path) -> Self {
        Self { dirs: self.dirs.clone(), program: Some(program.to_path_buf()) }
    }

    /// A round of extensions searches every library before the next round starts, so a copybook in
    /// any library is found before a program source (assumptions C85 to C87).
    fn find(&self, name: &str, library: Option<&str>, literal: bool) -> Option<PathBuf> {
        let rounds = if literal { [BARE, COPYBOOKS, PROGRAM_SOURCES] } else { [COPYBOOKS, PROGRAM_SOURCES, BARE] };
        self.find_in_rounds(name, library, &rounds)
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
}

fn copy_statement(words: &[Word], at: usize, pos: Pos) -> Result<Statement, Error> {
    let err = |m: &str| Error::at(pos, format!("COPY: {m}"));
    let text = |i: usize| words.get(i).map(|w| w.text.as_str());
    let mut i = at + 1;
    let quoted = |s: &str| s.starts_with(['\'', '"']);
    let unquote = |s: &str| s.trim_matches(|c| c == '\'' || c == '"').to_owned();
    // In `COPY X..` the separator period is the second, so the name is `X.` (assumption C88).
    let word = |s: &str| {
        if s.ends_with('.') && !quoted(s) {
            return Err(Error::at(pos, format!("COPY {s}: the name ends in a period; the period that ends a COPY statement is the one followed by a space")));
        }
        Ok(unquote(s))
    };
    let first = text(i).ok_or_else(|| err("a member name"))?;
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
        i += 1;
        while text(i).is_some_and(|w| w != ".") {
            let mode = match text(i) {
                Some(w) if w.eq_ignore_ascii_case("LEADING") => Mode::Leading,
                Some(w) if w.eq_ignore_ascii_case("TRAILING") => Mode::Trailing,
                _ => Mode::Whole,
            };
            if !matches!(mode, Mode::Whole) {
                i += 1;
            }
            let (pattern, after) = operand(words, i).ok_or_else(|| err("an operand to replace"))?;
            if !text(after).is_some_and(|w| w.eq_ignore_ascii_case("BY")) {
                return Err(err("BY"));
            }
            let (replacement, after) = operand(words, after + 1).ok_or_else(|| err("an operand after BY"))?;
            if pattern.is_empty() || (!matches!(mode, Mode::Whole) && pattern.len() != 1) {
                return Err(err("LEADING and TRAILING take one word; an empty pattern matches nothing"));
            }
            replacing.push(Replacing { mode, pattern, replacement: replacement.join(" ") });
            i = after;
        }
    }
    if text(i) != Some(".") {
        return Err(err("a period to end the statement"));
    }
    Ok(Statement { name, literal, library, replacing, next: i + 1 })
}

/// A REPLACING operand: pseudo-text `==...==` as its text-words, or one text-word.
fn operand(words: &[Word], at: usize) -> Option<(Vec<String>, usize)> {
    let first = words.get(at)?;
    if first.text != "==" {
        return Some((vec![first.text.clone()], at + 1));
    }
    let close = words[at + 1..].iter().position(|w| w.text == "==")? + at + 1;
    Some((words[at + 1..close].iter().map(|w| w.text.clone()).collect(), close + 1))
}

/// Appends `chars[range]` to `out` with the positions they came from.
fn copy_span(out: &mut Source, chars: &[char], positions: &[Pos], range: std::ops::Range<usize>) {
    out.text.extend(&chars[range.clone()]);
    out.positions.extend_from_slice(&positions[range]);
}

fn apply(src: &Source, replacing: &[Replacing]) -> Source {
    if replacing.is_empty() {
        return Source { text: src.text.clone(), positions: src.positions.clone(), options: Vec::new(), debugging: None };
    }
    let chars: Vec<char> = src.text.chars().collect();
    let words = text_words(&chars);
    let mut out = Source { text: String::new(), positions: Vec::new(), options: Vec::new(), debugging: None };
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
    let mut out = Source { text: String::new(), positions: Vec::new(), options: source.options.clone(), debugging: source.debugging.clone() };
    let read = |text: &str, file: u16| if source.debugging.is_some() { source::read_file_debugging(text, file) } else { source::read_file(text, file) };
    let (mut cursor, mut i) = (0usize, 0usize);
    while i < words.len() {
        let pos = source.positions[words[i].start];
        let (name, literal, library, replacing, next, sql) = if words[i].text.eq_ignore_ascii_case("COPY") {
            let st = copy_statement(&words, i, pos)?;
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
            (Some(path), _) => (path.display().to_string(), read_member(&path, pos, files, &read)?),
            (None, Some((path, mapset))) => {
                let mapset = mapset.map_err(|e| Error::at(pos, format!("{verb} {name}: {}", e.place(&path.display().to_string()))))?;
                let file = u16::try_from(files.len()).map_err(|_| Error::at(pos, "more than 65535 copy members"))?;
                files.push(path.display().to_string());
                (path.display().to_string(), read(&bms::symbolic_map(&mapset), file)?)
            }
            (None, None) => {
                let text = system::member(&name).ok_or_else(|| Error::at(pos, format!("{verb} {name}: no such member in the copy libraries")))?;
                let key = format!("(system member {})", name.to_ascii_uppercase());
                let file = u16::try_from(files.len()).map_err(|_| Error::at(pos, "more than 65535 copy members"))?;
                files.push(key.clone());
                (key, read(&text, file)?)
            }
        };
        if stack.contains(&key) || stack.len() >= MAX_DEPTH {
            return Err(Error::at(pos, format!("{verb} {name}: copies itself, or nests deeper than {MAX_DEPTH}")));
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

fn read_member(path: &Path, pos: Pos, files: &mut Vec<String>, read: &dyn Fn(&str, u16) -> Result<Source, Error>) -> Result<Source, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::at(pos, format!("COPY {}: {e}", path.display())))?;
    let file = u16::try_from(files.len()).map_err(|_| Error::at(pos, "more than 65535 copy members"))?;
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
}
