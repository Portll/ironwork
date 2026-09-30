//! Fixed-format reference format: columns 1-6 sequence, 7 indicator, 8-72 program text, 73 on
//! ignored. The result is one logical text with continuation lines joined, and the options of any
//! CBL or PROCESS cards ahead of the program.

use crate::{Error, Pos};

pub struct Source {
    pub text: String,
    /// For each char of `text`, where it came from.
    pub positions: Vec<Pos>,
    pub options: Vec<String>,
    /// None when debugging lines (D in column 7) were read as comments; else the file and line of
    /// each one read as program text.
    pub debugging: Option<Vec<(u16, u32)>>,
}

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
    read_lines(input, file, false)
}

/// Reads one file's text with its debugging lines as program text.
pub fn read_file_debugging(input: &str, file: u16) -> Result<Source, Error> {
    read_lines(input, file, true)
}

fn read_lines(input: &str, file: u16, debugging: bool) -> Result<Source, Error> {
    let mut out = Source { text: String::new(), positions: Vec::new(), options: Vec::new(), debugging: debugging.then(Vec::new) };
    let mut seen_program = false;
    let mut open_quote: Option<char> = None;
    let mut closed_at_72: Option<char> = None;
    let (mut identification, mut comment_entry) = (false, false);
    for (index, raw) in input.lines().enumerate() {
        let line = index as u32 + 1;
        let chars: Vec<char> = raw.trim_end_matches('\r').chars().map(|c| if c == '\t' { ' ' } else { c }).collect();
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
            if let Some(entering) = division_header(&area) {
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
    Ok(out)
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
}
