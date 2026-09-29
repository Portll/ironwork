//! Fixed-format reference format: columns 1-6 sequence, 7 indicator, 8-72 program text, 73 on
//! ignored. The result is one logical text with continuation lines joined, and the options of any
//! CBL or PROCESS cards ahead of the program.

use crate::{Error, Pos};

pub struct Source {
    pub text: String,
    /// For each char of `text`, where it came from.
    pub positions: Vec<Pos>,
    pub options: Vec<String>,
}

const TEXT_START: usize = 7;
const TEXT_END: usize = 72;

pub fn read(input: &str) -> Result<Source, Error> {
    read_file(input, 0)
}

/// Reads one file's text; `file` indexes its name in the program's file table.
pub fn read_file(input: &str, file: u16) -> Result<Source, Error> {
    let mut out = Source { text: String::new(), positions: Vec::new(), options: Vec::new() };
    let mut seen_program = false;
    let mut open_quote: Option<char> = None;
    for (index, raw) in input.lines().enumerate() {
        let line = index as u32 + 1;
        let chars: Vec<char> = raw.trim_end_matches('\r').chars().map(|c| if c == '\t' { ' ' } else { c }).collect();
        let body: String = chars.iter().take(TEXT_END).collect();
        if !seen_program && let Some(options) = option_card(&body) {
            out.options.extend(options);
            continue;
        }
        let indicator = chars.get(6).copied().unwrap_or(' ');
        if matches!(indicator, '*' | '/' | 'D' | 'd') {
            continue;
        }
        let area: Vec<char> = chars.iter().take(TEXT_END).skip(TEXT_START).copied().collect();
        if area.iter().all(|c| *c == ' ') {
            continue;
        }
        seen_program = true;
        let start_col = TEXT_START as u32 + 1;
        if indicator == '-' {
            let first = area.iter().position(|c| *c != ' ').unwrap();
            let skip = match open_quote {
                Some(q) if area[first] == q => first + 1,
                Some(_) => return Err(Error::at(Pos { file, line, col: start_col + first as u32 }, "a continued literal must resume with its quote")),
                None => {
                    while out.text.ends_with(' ') {
                        out.text.pop();
                        out.positions.pop();
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
            out.text.push('\n');
            out.positions.push(Pos { file, line, col: 0 });
            for (i, &c) in area.iter().enumerate() {
                if floating_comment(&area, i, open_quote) {
                    break;
                }
                push(&mut out, c, Pos { file, line, col: start_col + i as u32 }, &mut open_quote);
            }
            if open_quote.is_some() {
                for i in area.len()..TEXT_END - TEXT_START {
                    push(&mut out, ' ', Pos { file, line, col: start_col + i as u32 }, &mut open_quote);
                }
            }
        }
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
    let rest = trimmed.strip_prefix("CBL ").or_else(|| trimmed.strip_prefix("PROCESS ")).or_else(|| (trimmed == "CBL" || trimmed == "PROCESS").then_some(""))?;
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
    fn positions_point_at_the_source() {
        let s = read("       IDENTIFICATION DIVISION.\n").unwrap();
        let i = s.text.find('D').unwrap();
        assert_eq!(s.positions[i], Pos { file: 0, line: 1, col: 9 });
    }
}
