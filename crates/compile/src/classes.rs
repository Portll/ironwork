//! SPECIAL-NAMES CLASS clauses: each class-name's characters as a set of bytes in the program's
//! code page, for the class condition (Language Reference SC27-8713-03, pp. 128-129 and 269).

use syntax::Error;
use syntax::ast::{ClassClause, Literal};
use zarch::ebcdic::CodePage;

/// Each class-name with its characters, one bit per byte value.
pub(crate) fn sets(clauses: &[ClassClause], page: &CodePage, errors: &mut Vec<Error>) -> Vec<(String, [u8; 32])> {
    clauses
        .iter()
        .map(|clause| {
            let mut bits = [0u8; 32];
            for (first, last) in &clause.members {
                match members(first, last.as_ref(), page) {
                    Ok(bytes) => bytes.into_iter().for_each(|b| bits[usize::from(b / 8)] |= 1 << (b % 8)),
                    Err(m) => errors.push(Error::at(clause.pos, format!("CLASS {}: {m}", clause.name))),
                }
            }
            (clause.name.clone(), bits)
        })
        .collect()
}

/// The bytes a literal, or a THROUGH range, stands for. An ordinal number n is the character of
/// code point n - 1, in the code page's order ([`numeric::assumptions::CLASS_ORDINALS`]).
fn members(first: &Literal, last: Option<&Literal>, page: &CodePage) -> Result<Vec<u8>, String> {
    let from = bytes(first, page)?;
    let Some(last) = last else { return Ok(from) };
    let to = bytes(last, page)?;
    if matches!(first, Literal::Number(_)) != matches!(last, Literal::Number(_)) {
        return Err("the literals of a THROUGH phrase are both numeric or both alphanumeric".into());
    }
    match (from.as_slice(), to.as_slice()) {
        (&[a], &[b]) => Ok((a.min(b)..=a.max(b)).collect()),
        _ => Err("an alphanumeric literal of a THROUGH phrase is one character".into()),
    }
}

fn bytes(literal: &Literal, page: &CodePage) -> Result<Vec<u8>, String> {
    match literal {
        Literal::Alnum(text) if !text.is_empty() => page.encode(text).map_err(|e| format!("{text:?}: {e}")),
        Literal::Hex(bytes) if !bytes.is_empty() => Ok(bytes.clone()),
        Literal::Number(n) => match n.parse::<u16>() {
            Ok(ordinal @ 1..=256) if n.bytes().all(|b| b.is_ascii_digit()) => Ok(vec![(ordinal - 1) as u8]),
            _ => Err(format!("{n} is not an ordinal number from 1 to 256")),
        },
        _ => Err("the literals are alphanumeric characters or ordinal numbers".into()),
    }
}
