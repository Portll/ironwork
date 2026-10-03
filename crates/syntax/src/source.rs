//! Fixed-format reference format: columns 1-6 sequence, 7 indicator, 8-72 program text, 73 on
//! ignored. The result is one logical text with continuation lines joined, and the options of any
//! CBL or PROCESS cards ahead of the program. Under `--compliance extended` a file may be read in
//! free form instead (docs/compliance.md).

use crate::{Error, Pos};
use numeric::Compliance;

pub struct Source {
    pub text: String,
    /// For each char of `text`, where it came from.
    pub positions: Vec<Pos>,
    pub options: Vec<String>,
    /// None when debugging lines (D in column 7) were read as comments; else the file and line of
    /// each one read as program text.
    pub debugging: Option<Vec<(u16, u32)>>,
    /// The lines read in free form.
    pub free: Vec<FreeSpan>,
}

/// Lines `first` to `last` of file `file`, read in free form, and the warning that says so; a
/// COPY member read in free form as the line that copies it was has none of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreeSpan {
    pub file: u16,
    pub first: u32,
    pub last: u32,
    pub warning: Option<Error>,
}

impl FreeSpan {
    pub fn holds(&self, pos: Pos) -> bool {
        self.file == pos.file && (self.first..=self.last).contains(&pos.line)
    }
}

impl Source {
    /// The free-form span position `pos` is in, by its index in [`Source::free`].
    pub fn free_at(&self, pos: Pos) -> Option<usize> {
        self.free.iter().position(|s| s.holds(pos))
    }
}

/// The stable identifier and text of the warning for free-form source.
pub const FREE_FORM: &str = "IWX0001-W free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone)";

const TEXT_START: usize = 7;
const AREA_B: usize = 11;
const TEXT_END: usize = 72;

/// The IDENTIFICATION DIVISION paragraphs whose entry is a comment-entry (LR SC27-8713-03 p. 117),
/// and REMARKS, which Enterprise COBOL does not have (numeric::assumptions::COMMENT_ENTRY_REMARKS).
const COMMENT_PARAGRAPHS: &[&str] = &["AUTHOR", "INSTALLATION", "DATE-WRITTEN", "DATE-COMPILED", "SECURITY", "REMARKS"];

pub fn read(input: &str) -> Result<Source, Error> {
    read_file(input, 0)
}

/// Reads one file's text; `file` indexes its name in the program's file table. A comment-entry is
/// left out of the text, so neither COPY nor the lexer sees it (LR pp. 117, 700).
pub fn read_file(input: &str, file: u16) -> Result<Source, Error> {
    read_lines(input, file, false, false, false)
}

/// Reads one file's text with its debugging lines as program text.
pub fn read_file_debugging(input: &str, file: u16) -> Result<Source, Error> {
    read_lines(input, file, true, false, false)
}

/// Reads one file's text under `compliance`, with its debugging lines as program text when
/// `debugging` is set.
pub fn read_under(input: &str, file: u16, debugging: bool, compliance: Compliance) -> Result<Source, Error> {
    read_lines(input, file, debugging, compliance == Compliance::Extended, false)
}

/// Reads a COPY member's text as [`read_under`] does, starting in free form when the line that
/// copies it is free form, as GnuCOBOL and Micro Focus carry the source format into a member.
pub fn read_copied(input: &str, file: u16, debugging: bool, compliance: Compliance, copied_free: bool) -> Result<Source, Error> {
    let extended = compliance == Compliance::Extended;
    read_lines(input, file, debugging, extended, extended && copied_free)
}

fn read_lines(input: &str, file: u16, debugging: bool, extended: bool, copied_free: bool) -> Result<Source, Error> {
    let mut out = Source { text: String::new(), positions: Vec::new(), options: Vec::new(), debugging: debugging.then(Vec::new), free: Vec::new() };
    let mut seen_program = false;
    let mut open_quote: Option<char> = None;
    let mut closed_at_72: Option<char> = None;
    let (mut identification, mut comment_entry) = (false, false);
    let lines: Vec<Vec<char>> = input.lines().map(|raw| raw.trim_end_matches('\r').chars().map(|c| if c == '\t' { ' ' } else { c }).collect()).collect();
    let mut free = match (extended, copied_free) {
        (true, true) => Some((1, None)),
        (true, false) => free_from_the_start(input, file).map(|(first, warning)| (first, Some(warning))),
        (false, _) => None,
    };
    for (index, chars) in lines.iter().enumerate() {
        let line = index as u32 + 1;
        if extended && let Some(found) = directive(chars, Pos { file, line, col: 1 }) {
            let (format, pos) = found?;
            match format {
                Format::Free if free.is_none() => free = Some((line + 1, Some(Error::warning(pos, format!("{FREE_FORM}: this directive makes the lines after it free form"))))),
                Format::Fixed => {
                    if let Some((first, warning)) = free.take().filter(|(first, _)| *first < line) {
                        out.free.push(FreeSpan { file, first, last: line - 1, warning });
                    }
                }
                Format::Free => {}
            }
            continue;
        }
        if free.is_some() {
            if !seen_program && let Some(options) = option_card(&chars.iter().collect::<String>()) {
                out.options.extend(options);
                continue;
            }
            let Some((start, debugging_line)) = free_line(chars) else { continue };
            if debugging_line && out.debugging.is_none() {
                continue;
            }
            let area = &chars[start..];
            if area.iter().all(|c| *c == ' ') {
                continue;
            }
            if open_quote.is_some() {
                return Err(Error::at(Pos { file, line, col: 1 }, "a literal runs to the end of the line with no continuation"));
            }
            if debugging_line && let Some(lines) = &mut out.debugging {
                lines.push((file, line));
            }
            seen_program = true;
            comment_entry = false;
            closed_at_72 = None;
            if listing_control(area) {
                continue;
            }
            if let Some(entering) = division_header(area).or(program_id_first(area).then_some(true)) {
                identification = entering;
            }
            let header_end = if identification { comment_paragraph(area) } else { None };
            out.text.push('\n');
            out.positions.push(Pos { file, line, col: 0 });
            for (i, &c) in area.iter().enumerate().take(header_end.map_or(area.len(), |period| period + 1)) {
                if floating_comment(area, i, open_quote) {
                    break;
                }
                push(&mut out, c, Pos { file, line, col: (start + i) as u32 + 1 }, &mut open_quote);
            }
            continue;
        }
        let body: String = chars.iter().take(TEXT_END).collect();
        if !seen_program && let Some(options) = option_card(&body) {
            out.options.extend(options);
            continue;
        }
        let indicator = chars.get(6).copied().unwrap_or(' ');
        let debugging_line = matches!(indicator, 'D' | 'd');
        if indicator == '*' || indicator == '/' || (debugging_line && out.debugging.is_none()) {
            continue;
        }
        let area: Vec<char> = chars.iter().take(TEXT_END).skip(TEXT_START).copied().collect();
        if area.iter().all(|c| *c == ' ') {
            continue;
        }
        if debugging_line && let Some(lines) = &mut out.debugging {
            lines.push((file, line));
        }
        seen_program = true;
        let area_a_blank = area.iter().take(AREA_B - TEXT_START).all(|c| *c == ' ');
        if comment_entry && area_a_blank {
            continue;
        }
        comment_entry = false;
        if indicator != '-' && open_quote.is_none() && listing_control(&area) {
            continue;
        }
        let start_col = TEXT_START as u32 + 1;
        if indicator == '-' {
            let first = area.iter().position(|c| *c != ' ').unwrap();
            let skip = match open_quote {
                Some(q) if area[first] == q => first + 1,
                Some(_) => return Err(Error::at(Pos { file, line, col: start_col + first as u32 }, "a continued literal must resume with its quote")),
                // A closing quote in column 72 and the continuation's first two quotes are one doubled
                // quote (LR p. 58).
                None if closed_at_72.is_some_and(|q| area[first] == q && area.get(first + 1) == Some(&q)) => first + 1,
                None => {
                    while out.text.ends_with(' ') {
                        out.text.pop();
                        out.positions.pop();
                    }
                    // After a closed literal, a quote starts a second literal (LR p. 58).
                    if matches!(area[first], '\'' | '"') && out.text.ends_with(['\'', '"']) {
                        out.text.push(' ');
                        out.positions.push(Pos { file, line, col: start_col + first as u32 });
                    }
                    first
                }
            };
            for (i, &c) in area.iter().enumerate().skip(skip) {
                if floating_comment(&area, i, open_quote) {
                    break;
                }
                push(&mut out, c, Pos { file, line, col: start_col + i as u32 }, &mut open_quote);
            }
        } else {
            if open_quote.is_some() {
                return Err(Error::at(Pos { file, line, col: 1 }, "a literal runs to the end of the line with no continuation"));
            }
            if let Some(entering) = division_header(&area).or((extended && program_id_first(&area)).then_some(true)) {
                identification = entering;
            }
            let header_end = if identification { comment_paragraph(&area) } else { None };
            out.text.push('\n');
            out.positions.push(Pos { file, line, col: 0 });
            for (i, &c) in area.iter().enumerate().take(header_end.map_or(area.len(), |period| period + 1)) {
                if floating_comment(&area, i, open_quote) {
                    break;
                }
                push(&mut out, c, Pos { file, line, col: start_col + i as u32 }, &mut open_quote);
            }
            comment_entry = header_end.is_some();
            if open_quote.is_some() {
                for i in area.len()..TEXT_END - TEXT_START {
                    push(&mut out, ' ', Pos { file, line, col: start_col + i as u32 }, &mut open_quote);
                }
            }
        }
        closed_at_72 = match (out.text.chars().next_back(), out.positions.last()) {
            (Some(q @ ('\'' | '"')), Some(p)) if open_quote.is_none() && p.line == line && p.col == TEXT_END as u32 => Some(q),
            _ => None,
        };
    }
    if open_quote.is_some() {
        return Err(Error::at(out.positions.last().copied().unwrap_or_default(), "an unterminated literal"));
    }
    if let Some((first, warning)) = free {
        out.free.push(FreeSpan { file, first, last: u32::MAX, warning });
    }
    Ok(out)
}

/// A file is free form from its first line when a line before any source-format directive cannot
/// be fixed form: its text starts in columns 1 to 6, with a character other than a digit, and runs
/// on through column 7 with a character other than a space or an indicator (`*`, `/`, `-`, `D`,
/// `d`). A tab advances to the next column after a multiple of 8 here, as GnuCOBOL and Micro Focus
/// place it. The warning is at column 7.
fn free_from_the_start(input: &str, file: u16) -> Option<(u32, Error)> {
    for (index, raw) in input.lines().enumerate() {
        let line = index as u32 + 1;
        let mut chars = Vec::new();
        for c in raw.trim_end_matches('\r').chars() {
            match c {
                '\t' => chars.resize((chars.len() / 8 + 1) * 8, ' '),
                c => chars.push(c),
            }
        }
        if directive(&chars, Pos { file, line, col: 1 }).is_some() {
            return None;
        }
        let (Some(start), Some(&c)) = (chars.iter().position(|c| *c != ' '), chars.get(TEXT_START - 1)) else { continue };
        let card = option_card(&chars.iter().take(TEXT_END).collect::<String>()).is_some();
        if start < TEXT_START - 1 && !chars[start].is_ascii_digit() && !matches!(c, ' ' | '*' | '/' | '-' | 'D' | 'd') && !card {
            let why = format!("{FREE_FORM}: column 7 holds {c:?}, which no fixed-form line can, so the file is read in free form");
            return Some((1, Error::warning(Pos { file, line, col: TEXT_START as u32 }, why)));
        }
    }
    None
}

/// The column a free-form line's text starts at and whether it is a debugging line, or None for a
/// comment line: `*` or `/` in column 1 makes a comment line, and `D` followed by a space a
/// debugging line, as in Micro Focus's free format. GnuCOBOL refuses such lines unless they begin
/// `*>`.
fn free_line(chars: &[char]) -> Option<(usize, bool)> {
    match (chars.first(), chars.get(1)) {
        (Some('*' | '/'), _) => None,
        (Some('D' | 'd'), Some(' ')) => Some((1, true)),
        _ => Some((0, false)),
    }
}

enum Format {
    Free,
    Fixed,
}

/// A compiler directive, alone on its line, and where it starts: `>>` first on the line, or `$`
/// first in column 1 or 7, Micro Focus's directive indicator. Err for any but a source-format
/// directive: `>>SOURCE [FORMAT] [IS] FREE|FIXED`, or `$SET` or `>>SET` with `SOURCEFORMAT"FREE"`,
/// `SOURCEFORMAT"FIXED"` or `SOURCEFORMAT(FREE)`.
fn directive(chars: &[char], pos: Pos) -> Option<Result<(Format, Pos), Error>> {
    let start = chars.iter().position(|c| *c != ' ')?;
    let text: String = chars[start..].iter().collect();
    let words = if let Some(rest) = text.strip_prefix(">>") {
        rest
    } else if text.starts_with('$') && (start == 0 || start == TEXT_START - 1) {
        &text[1..]
    } else {
        return None;
    };
    let upper = words.trim().to_ascii_uppercase();
    let words: Vec<&str> = upper.split_whitespace().filter(|w| !matches!(*w, "FORMAT" | "IS")).collect();
    let format = |value: &str| match value.trim_matches(|c| matches!(c, '"' | '\'' | '(' | ')')) {
        "FREE" => Some(Format::Free),
        "FIXED" => Some(Format::Fixed),
        _ => None,
    };
    let found = match words.as_slice() {
        ["SOURCE", value] => format(value),
        ["SET", setting] => setting.strip_prefix("SOURCEFORMAT").and_then(format),
        _ => None,
    };
    let pos = Pos { col: start as u32 + 1, ..pos };
    let shown = text.trim_end();
    Some(found.map(|f| (f, pos)).ok_or_else(|| Error::at(pos, format!("{shown}: the source-format directives >>SOURCE and $SET SOURCEFORMAT, giving FREE or FIXED, are the only compiler directives ironwork reads"))))
}

/// EJECT, SKIP1, SKIP2, SKIP3 or TITLE with its literal, alone on the line and perhaps ended by a
/// period, and perhaps by a floating comment: statements for the listing that have no effect on
/// compilation.
fn listing_control(area: &[char]) -> bool {
    let mut quote = None;
    let end = (0..area.len())
        .find(|&i| {
            let comment = floating_comment(area, i, quote);
            match (quote, area[i]) {
                (None, c @ ('\'' | '"')) => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                _ => {}
            }
            comment
        })
        .unwrap_or(area.len());
    let line: String = area[..end].iter().collect();
    let line = line.trim();
    let line = line.strip_suffix('.').unwrap_or(line).trim_end();
    if ["EJECT", "SKIP1", "SKIP2", "SKIP3"].iter().any(|w| line.eq_ignore_ascii_case(w)) {
        return true;
    }
    let Some(literal) = line.get(..6).filter(|t| t.eq_ignore_ascii_case("TITLE ")).map(|_| line[6..].trim_start()) else { return false };
    let literal = literal.strip_prefix(['N', 'n', 'G', 'g']).filter(|l| l.starts_with(['\'', '"'])).unwrap_or(literal);
    let Some(quote) = literal.chars().next().filter(|c| matches!(c, '\'' | '"')) else { return false };
    let inner = literal.get(1..literal.len() - 1).unwrap_or_default();
    literal.len() >= 2 && literal.ends_with(quote) && !inner.replace(&format!("{quote}{quote}"), "").contains(quote)
}

/// `*>` outside a literal starts a comment that runs to the end of the line.
fn floating_comment(area: &[char], i: usize, open_quote: Option<char>) -> bool {
    open_quote.is_none() && area[i] == '*' && area.get(i + 1) == Some(&'>')
}

/// The word that starts at or after `from`, uppercased, and the index just past it.
fn word_from(area: &[char], from: usize) -> Option<(String, usize)> {
    let start = from + area.get(from..)?.iter().position(|c| *c != ' ')?;
    let len = area[start..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '-' || **c == '_').count();
    (len > 0).then(|| (area[start..start + len].iter().collect::<String>().to_ascii_uppercase(), start + len))
}

/// Whether a line starts with PROGRAM-ID, which begins the IDENTIFICATION DIVISION of a program
/// whose header `--compliance extended` lets it leave out.
fn program_id_first(area: &[char]) -> bool {
    word_from(area, 0).is_some_and(|(word, _)| word == "PROGRAM-ID")
}

/// For a line that starts with a division header, whether it is the IDENTIFICATION DIVISION's.
fn division_header(area: &[char]) -> Option<bool> {
    let (first, end) = word_from(area, 0)?;
    let (second, _) = word_from(area, end)?;
    (second == "DIVISION").then(|| first == "IDENTIFICATION" || first == "ID")
}

/// Where the period is, for a line that starts with a paragraph header whose entry is a
/// comment-entry.
fn comment_paragraph(area: &[char]) -> Option<usize> {
    let (word, end) = word_from(area, 0)?;
    let period = end + area[end..].iter().position(|c| *c != ' ')?;
    (area[period] == '.' && COMMENT_PARAGRAPHS.contains(&word.as_str())).then_some(period)
}

fn push(out: &mut Source, c: char, pos: Pos, open_quote: &mut Option<char>) {
    match *open_quote {
        Some(q) if c == q => *open_quote = None,
        None if c == '\'' || c == '"' => *open_quote = Some(c),
        _ => {}
    }
    out.text.push(c);
    out.positions.push(pos);
}

/// A CBL or PROCESS card: its options, split at commas and spaces outside parentheses.
fn option_card(line: &str) -> Option<Vec<String>> {
    let sequence: String = line.chars().take(6).collect();
    let line = if sequence.len() == 6 && sequence.chars().all(|c| c.is_ascii_digit() || c == ' ') { &line[6..] } else { line };
    let trimmed = line.trim_start();
    let keyword = trimmed.split(' ').next().unwrap_or("");
    let rest = (keyword.eq_ignore_ascii_case("CBL") || keyword.eq_ignore_ascii_case("PROCESS")).then(|| &trimmed[keyword.len()..])?;
    let (mut options, mut current, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in rest.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && (c == ',' || c == ' ') {
            if !current.is_empty() {
                options.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        options.push(current);
    }
    Some(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_cards_ahead_of_the_program_are_collected() {
        let s = read("       CBL TRUNC(OPT),NUMPROC(PFD) ARITH(EXTEND)\n       PROCESS SSRANGE\n       IDENTIFICATION DIVISION.\n").unwrap();
        assert_eq!(s.options, ["TRUNC(OPT)", "NUMPROC(PFD)", "ARITH(EXTEND)", "SSRANGE"]);
        assert_eq!(s.text.trim(), "IDENTIFICATION DIVISION.");
    }

    #[test]
    fn a_suboption_list_stays_whole() {
        assert_eq!(option_card("CBL FLAG(I,W),X").unwrap(), ["FLAG(I,W)", "X"]);
    }

    #[test]
    fn sequence_numbers_comments_and_columns_past_72_are_dropped() {
        let s = read("000100 IDENTIFICATION DIVISION.                                         SEQ00001\n000200*a comment\n000300/page\n").unwrap();
        assert_eq!(s.text.trim(), "IDENTIFICATION DIVISION.");
    }

    #[test]
    fn listing_statements_alone_on_their_line_are_left_out() {
        let text = [
            "       01  A PIC X.\n",
            "           EJECT\n",
            "       SKIP2.\n",
            "           TITLE 'RATES, ''A'' TO Z'.\n",
            "           title n\"NATIONAL\"\n",
            "       01  B PIC X.\n",
            "           EJECT X\n",
        ]
        .concat();
        let s = read(&text).unwrap();
        assert_eq!(s.text.split_whitespace().collect::<Vec<_>>(), ["01", "A", "PIC", "X.", "01", "B", "PIC", "X.", "EJECT", "X"]);
    }

    #[test]
    fn a_continued_literal_keeps_its_trailing_spaces() {
        let first = format!("       01 A PIC X(70) VALUE 'ABC{}", " ".repeat(72 - 34));
        let text = format!("{first}\n      -    'DEF'.\n");
        let s = read(&text).unwrap();
        let literal = &s.text[s.text.find('\'').unwrap()..];
        assert_eq!(literal.trim_end(), format!("'ABC{}DEF'.", " ".repeat(40)));
    }

    #[test]
    fn an_option_card_may_be_lowercase() {
        assert_eq!(read(" cbl dll,thread\n process ssrange\n").unwrap().options, ["dll", "thread", "ssrange"]);
    }

    #[test]
    fn an_option_card_may_carry_a_sequence_number() {
        assert_eq!(read("000010 CBL ARITH(EXTEND)\n").unwrap().options, ["ARITH(EXTEND)"]);
    }

    #[test]
    fn a_continued_word_joins_without_a_space() {
        let s = read("           MOVE ABC\n      -    DEF TO X.\n").unwrap();
        assert!(s.text.contains("MOVE ABCDEF TO X."));
    }

    #[test]
    fn a_floating_comment_ends_the_line_but_not_inside_a_literal() {
        let s = read("           MOVE 1 TO X *> set X\n           DISPLAY '*> kept'\n").unwrap();
        assert!(s.text.contains("MOVE 1 TO X") && !s.text.contains("set X"));
        assert!(s.text.contains("'*> kept'"));
    }

    #[test]
    fn a_comment_entry_is_left_out_of_the_text() {
        let s = read(concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. CE1.\n",
            "       AUTHOR. James O'Grady & Sons @ ACME.\n",
            "       security.\n",
            "           THIS PROGRAM CHECKS THE COMPILER\"S ABILITY.\n",
            "\n",
            "      * a comment line\n",
            "      -    A HYPHEN IN COLUMN 7.\n",
            "           COPY NOTHERE.\n",
            "       DATE-COMPILED.\n",
            "       ENVIRONMENT DIVISION.\n",
        ))
        .unwrap();
        let words: Vec<&str> = s.text.split_whitespace().collect();
        assert_eq!(words, ["IDENTIFICATION", "DIVISION.", "PROGRAM-ID.", "CE1.", "AUTHOR.", "security.", "DATE-COMPILED.", "ENVIRONMENT", "DIVISION."]);
    }

    #[test]
    fn comment_entries_belong_to_the_identification_division_alone() {
        let s = read(concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. P.\n",
            "       REMARKS. KEPT OUT.\n",
            "       PROCEDURE DIVISION.\n",
            "       REMARKS.\n",
            "           DISPLAY 'IT''S'.\n",
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. INNER.\n",
            "       AUTHOR. O'GRADY.\n",
        ))
        .unwrap();
        assert!(!s.text.contains("KEPT OUT") && !s.text.contains("GRADY"), "{}", s.text);
        assert!(s.text.contains("DISPLAY 'IT''S'."), "{}", s.text);
    }

    #[test]
    fn a_quote_in_column_72_doubled_on_the_continuation_is_one_quote() {
        let head = "           MOVE \"";
        let first = format!("{head}{}\"", "A".repeat(TEXT_END - head.len() - 1));
        let s = read(&format!("{first}\n      -    \"\"B\" TO X.\n")).unwrap();
        assert!(s.text.contains(&format!("\"{}\"\"B\" TO X.", "A".repeat(TEXT_END - head.len() - 1))), "{}", s.text);
    }

    #[test]
    fn a_quote_after_a_closed_literal_starts_another() {
        let s = read("           88 V VALUE 'ABC'\n      -    'DEF'.\n").unwrap();
        assert!(s.text.contains("'ABC' 'DEF'."), "{}", s.text);
        let head = "           88 V VALUE '";
        let first = format!("{head}{}'", "A".repeat(TEXT_END - head.len() - 1));
        let s = read(&format!("{first}\n      -    'B'.\n")).unwrap();
        assert!(s.text.ends_with("A' 'B'."), "{}", s.text);
    }

    #[test]
    fn debugging_lines_are_comments_unless_read_as_text() {
        let text = "           MOVE 1 TO X\n      D    DISPLAY X\n      d    DISPLAY Y\n      D\n";
        let plain = read(text).unwrap();
        assert!(!plain.text.contains("DISPLAY") && plain.debugging.is_none());
        let debugging = read_file_debugging(text, 3).unwrap();
        assert!(debugging.text.contains("DISPLAY X") && debugging.text.contains("DISPLAY Y"));
        assert_eq!(debugging.debugging, Some(vec![(3, 2), (3, 3)]));
    }

    #[test]
    fn positions_point_at_the_source() {
        let s = read("       IDENTIFICATION DIVISION.\n").unwrap();
        let i = s.text.find('D').unwrap();
        assert_eq!(s.positions[i], Pos { file: 0, line: 1, col: 9 });
    }

    fn extended(text: &str) -> Result<Source, Error> {
        read_under(text, 0, false, Compliance::Extended)
    }

    #[test]
    fn a_file_that_cannot_be_fixed_form_is_read_in_free_form_under_extended_alone() {
        let text = "*\nIDENTIFICATION DIVISION.\n* a comment line\n*> another\nPROGRAM-ID. F.\n    DISPLAY 'A *> B' *> gone\n";
        let s = extended(text).unwrap();
        let words: Vec<&str> = s.text.split_whitespace().collect();
        assert_eq!(words, ["IDENTIFICATION", "DIVISION.", "PROGRAM-ID.", "F.", "DISPLAY", "'A", "*>", "B'"]);
        assert_eq!(s.free.len(), 1);
        let warning = s.free[0].warning.clone().unwrap();
        assert_eq!((s.free[0].first, s.free[0].last, warning.pos), (1, u32::MAX, Pos { file: 0, line: 2, col: 7 }));
        assert!(warning.message.starts_with("IWX0001-W free-form source") && warning.message.contains("column 7 holds 'F'"));
        assert_eq!(s.positions[s.text.find('I').unwrap()], Pos { file: 0, line: 2, col: 1 });
        let strict = read(text).unwrap();
        assert!(strict.free.is_empty() && strict.text.contains("ICATION DIVISION."), "{}", strict.text);
    }

    #[test]
    fn a_free_form_line_runs_past_column_72_and_has_no_continuation() {
        let long = format!("01 A PIC X(80) VALUE '{}'.", "Z".repeat(70));
        let s = extended(&format!("IDENTIFICATION DIVISION.\n{long}\n")).unwrap();
        assert!(s.text.contains(&long));
        let Err(open) = extended("IDENTIFICATION DIVISION.\n    DISPLAY 'AB\n-    'C'.\n") else { panic!("an open literal is refused") };
        assert_eq!((open.message.as_str(), open.pos.line), ("a literal runs to the end of the line with no continuation", 3));
    }

    #[test]
    fn a_d_and_a_space_in_column_1_is_a_debugging_line_and_a_comment_entry_ends_with_its_line() {
        let text = "IDENTIFICATION DIVISION.\nPROGRAM-ID. D.\nAUTHOR. A & B.\nD   DISPLAY 'DEBUG'\nDISPLAY 'KEPT'\n";
        assert!(!extended(text).unwrap().text.contains("DEBUG"));
        let debugging = read_under(text, 0, true, Compliance::Extended).unwrap();
        assert!(debugging.text.contains("DISPLAY 'DEBUG'") && debugging.text.contains("DISPLAY 'KEPT'") && !debugging.text.contains("A & B"));
        assert_eq!(debugging.debugging, Some(vec![(0, 4)]));
    }

    #[test]
    fn a_source_format_directive_switches_the_form_from_the_next_line() {
        let text = "      $SET SOURCEFORMAT\"FREE\"\nIDENTIFICATION DIVISION.\n>>SOURCE FORMAT IS FIXED\n000100 PROGRAM-ID. P.\n  >>source free\nDATA DIVISION.\n";
        let s = extended(text).unwrap();
        let words: Vec<&str> = s.text.split_whitespace().collect();
        assert_eq!(words, ["IDENTIFICATION", "DIVISION.", "PROGRAM-ID.", "P.", "DATA", "DIVISION."]);
        let spans: Vec<(u32, u32, Option<Pos>)> = s.free.iter().map(|f| (f.first, f.last, f.warning.as_ref().map(|w| w.pos))).collect();
        assert_eq!(spans, [(2, 2, Some(Pos { file: 0, line: 1, col: 7 })), (6, u32::MAX, Some(Pos { file: 0, line: 5, col: 3 }))]);
        let Err(other) = extended("       >>IF X DEFINED\n") else { panic!("another directive is refused") };
        assert!(other.message.starts_with(">>IF X DEFINED: the source-format directives"), "{}", other.message);
        assert_eq!(other.pos.col, 8);
    }

    #[test]
    fn a_fixed_form_file_stays_fixed_under_extended() {
        let text = concat!(
            "000100 IDENTIFICATION DIVISION.                                         SEQ00001\n",
            "007000C    LABEL RECORDS                                                SQ1054.2\n",
            "\t   RECORD IS VARYING\n",
            "       CBL APOST\n",
            "ABC123 MOVE A TO B.\n",
        );
        let s = extended(text).unwrap();
        assert!(s.free.is_empty());
        assert_eq!(s.text, read(text).unwrap().text);
    }

    #[test]
    fn a_member_copied_from_a_free_form_line_starts_free_with_no_warning_of_its_own() {
        let member = "    *> a member indented as free form\n    02 B PIC X(80) VALUE 'past column seventy-two, which free form keeps whole as it reads it'.\n";
        let copied = read_copied(member, 2, false, Compliance::Extended, true).unwrap();
        assert!(copied.text.contains("02 B PIC X(80)") && copied.text.trim_end().ends_with("whole as it reads it'."), "{}", copied.text);
        assert_eq!((copied.free.len(), copied.free[0].first, copied.free[0].warning.clone()), (1, 1, None));
        assert!(read_copied(member, 2, false, Compliance::Extended, false).is_err());
        assert!(read_copied(member, 2, false, Compliance::Strict, true).is_err());
    }

    #[test]
    fn listing_statements_alone_on_a_line_are_left_out() {
        let s = read(concat!(
            "       01  A PIC X.\n",
            "           EJECT\n",
            "       SKIP1\n",
            "           skip2.\n",
            "           SKIP3 *> blank lines\n",
            "           TITLE 'PAYROLL: PART 2'.\n",
            "           TITLE 'A *> B' *> the title holds *>\n",
            "       TITLE \"X\"\n",
            "       01  B PIC X.\n",
            "           MOVE TITLE TO EJECT.\n",
            "           EJECT X.\n",
        ))
        .unwrap();
        let words: Vec<&str> = s.text.split_whitespace().collect();
        assert_eq!(words, ["01", "A", "PIC", "X.", "01", "B", "PIC", "X.", "MOVE", "TITLE", "TO", "EJECT.", "EJECT", "X."]);
    }
}
