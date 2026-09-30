//! Debugging lines, D in column 7: program text in a program whose SOURCE-COMPUTER paragraph says
//! WITH DEBUGGING MODE, and in the programs it contains, whose configuration section it is;
//! comments anywhere else (Language Reference SC27-8713-03, pp. 121-122, 771-772).

use crate::lexer::{Tok, Token};
use std::collections::HashSet;

fn word(tokens: &[Token], i: usize) -> Option<&str> {
    match tokens.get(i).map(|t| &t.tok) {
        Some(Tok::Word(w)) => Some(w),
        _ => None,
    }
}

/// Whether the SOURCE-COMPUTER paragraph at `i` says WITH DEBUGGING MODE.
fn debugging_mode_at(tokens: &[Token], i: usize) -> bool {
    let paragraph = tokens[i + 1..].iter().skip_while(|t| t.tok == Tok::Period).take_while(|t| t.tok != Tok::Period);
    let words: Vec<&str> = paragraph.filter_map(|t| if let Tok::Word(w) = &t.tok { Some(w.as_str()) } else { None }).collect();
    words.windows(2).any(|w| w == ["DEBUGGING", "MODE"])
}

/// Whether any program in the source is compiled WITH DEBUGGING MODE.
pub(crate) fn requested(tokens: &[Token]) -> bool {
    (0..tokens.len()).any(|i| word(tokens, i) == Some("SOURCE-COMPUTER") && debugging_mode_at(tokens, i))
}

/// The tokens with those of each debugging line in `lines` dropped, unless the program they are in
/// is compiled WITH DEBUGGING MODE. An IDENTIFICATION DIVISION outside any other program starts a
/// program of its own, which has its own configuration section.
pub(crate) fn keep(tokens: Vec<Token>, lines: &[(u16, u32)]) -> Vec<Token> {
    let lines: HashSet<(u16, u32)> = lines.iter().copied().collect();
    let (mut depth, mut on) = (0usize, false);
    let mut kept = Vec::with_capacity(tokens.len());
    for (i, t) in tokens.iter().enumerate() {
        if lines.contains(&(t.pos.file, t.pos.line)) {
            if on {
                kept.push(t.clone());
            }
            continue;
        }
        match (word(&tokens, i), word(&tokens, i + 1)) {
            (Some("IDENTIFICATION" | "ID"), Some("DIVISION")) => {
                on &= depth > 0;
                depth += 1;
            }
            (Some("END"), Some("PROGRAM" | "CLASS" | "METHOD" | "FACTORY" | "OBJECT")) => depth = depth.saturating_sub(1),
            (Some("SOURCE-COMPUTER"), _) => on = debugging_mode_at(&tokens, i),
            _ => {}
        }
        kept.push(t.clone());
    }
    kept
}

#[cfg(test)]
mod tests {
    fn parse_all(text: &str) -> Vec<crate::ast::Program> {
        crate::parse_all_with(text, &crate::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"))
    }

    fn program(id: &str, mode: &str, body: &str) -> String {
        format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SOURCE-COMPUTER. IBM-370{mode}.\n       PROCEDURE DIVISION.\n           DISPLAY 'A'\n      D    DISPLAY 'D'\n{body}           GOBACK.\n"
        )
    }

    fn displays(p: &crate::ast::Program) -> usize {
        p.paragraphs[0].statements.iter().filter(|s| matches!(s, crate::ast::Stmt::Display { .. })).count()
    }

    #[test]
    fn debugging_lines_are_program_text_only_in_debugging_mode() {
        assert_eq!(displays(&parse_all(&program("T", "", ""))[0]), 1);
        let on = parse_all(&program("T", " WITH DEBUGGING MODE", ""));
        assert!(on[0].environment.debugging_mode);
        assert_eq!(displays(&on[0]), 2);
    }

    #[test]
    fn a_contained_program_shares_debugging_mode_and_a_separate_one_does_not() {
        let inner = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n      D    DISPLAY 'I'\n           GOBACK.\n       END PROGRAM INNER.\n";
        let text = [program("OUTER", " WITH DEBUGGING MODE", ""), inner.into(), "       END PROGRAM OUTER.\n".into(), program("NEXT", "", "")].concat();
        let all = parse_all(&text);
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["OUTER", "INNER", "NEXT"]);
        assert_eq!(displays(&all[1]), 1);
        assert!(all[1].environment.debugging_mode);
        assert_eq!(displays(&all[2]), 1);
    }
}
