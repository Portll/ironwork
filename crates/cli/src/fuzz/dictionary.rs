//! The values a program compares its data with, which fuzz gives a field now and then in place of
//! a generated one: a literal a relation compares the item with, an EVALUATE subject's WHEN
//! values, and the values of the item's condition-names.

use std::collections::{BTreeMap, BTreeSet};

use exec::layout::Resolved;
use rt::storage::Kind;
use rt::vocab::{Figurative, SignPosition};
use syntax::ast::{Cond, Expr, Literal, Loop, Object, Operand, Ref, Stmt, Subject};

use super::{Field, MINUS, PLUS, SPACE, ebcdic};

/// Each item's values by its index in the layout, as the literals the program writes them.
pub(crate) fn literals_by_item(compiled: &exec::Compiled) -> BTreeMap<usize, Vec<Literal>> {
    let layout = &compiled.layout;
    let mut by_item: BTreeMap<usize, Vec<Literal>> = BTreeMap::new();
    let mut add = |item: usize, literal: &Literal| {
        let values = by_item.entry(item).or_default();
        if !values.contains(literal) {
            values.push(literal.clone());
        }
    };
    for c in &layout.conditions {
        for (from, thru) in &c.values {
            add(c.item, from);
            if let Some(thru) = thru {
                add(c.item, thru);
            }
        }
    }
    let mut all = Vec::new();
    for p in &compiled.program.paragraphs {
        super::statements(&p.statements, &mut all);
    }
    let item_of = |r: &Ref| match layout.resolve(&r.name, &r.qualifiers, r.pos) {
        Ok(Resolved::Item(i)) => Some(i),
        _ => None,
    };
    let mut pairs: Vec<(usize, &Literal)> = Vec::new();
    for s in all {
        for cond in conditions(s) {
            compared(cond, &item_of, &mut pairs);
        }
        if let Stmt::Evaluate { subjects, whens, .. } = s {
            for alternative in whens.iter().flat_map(|w| &w.alternatives) {
                for (subject, object) in subjects.iter().zip(alternative) {
                    if let (Subject::Expr(Expr::Operand(Operand::Ref(r))), Object::Value { from, thru, .. }) = (subject, object)
                        && let Some(item) = item_of(r)
                    {
                        for e in std::iter::once(from).chain(thru) {
                            if let Expr::Operand(Operand::Literal(l)) = e {
                                pairs.push((item, l));
                            }
                        }
                    }
                }
            }
        }
    }
    for (item, literal) in pairs {
        add(item, literal);
    }
    by_item
}

/// The conditions a PERFORM's UNTIL phrases test.
fn looped(repeat: &Loop) -> Vec<&Cond> {
    match repeat {
        Loop::Until { cond, .. } => vec![cond],
        Loop::Varying { varying, after, .. } => std::iter::once(&varying.until).chain(after.iter().map(|v| &v.until)).collect(),
        _ => Vec::new(),
    }
}

/// The conditions a statement tests.
fn conditions(s: &Stmt) -> Vec<&Cond> {
    match s {
        Stmt::If { cond, .. } => vec![cond],
        Stmt::PerformInline { repeat, .. } | Stmt::PerformProc { repeat, .. } => looped(repeat),
        Stmt::Evaluate { subjects, whens, .. } => subjects
            .iter()
            .filter_map(|s| if let Subject::Cond(c) = s { Some(c) } else { None })
            .chain(whens.iter().flat_map(|w| &w.alternatives).flatten().filter_map(|o| if let Object::Cond(c) = o { Some(c) } else { None }))
            .collect(),
        Stmt::Search(search) => search.whens.iter().map(|(c, _)| c).collect(),
        _ => Vec::new(),
    }
}

/// Each item a relation in `cond` compares with a literal, with that literal.
fn compared<'a>(cond: &'a Cond, item_of: &dyn Fn(&Ref) -> Option<usize>, out: &mut Vec<(usize, &'a Literal)>) {
    match cond {
        Cond::Rel(a, _, b) => {
            let pair = match (a, b) {
                (Expr::Operand(Operand::Ref(r)), Expr::Operand(Operand::Literal(l))) | (Expr::Operand(Operand::Literal(l)), Expr::Operand(Operand::Ref(r))) => Some((r, l)),
                _ => None,
            };
            if let Some((r, l)) = pair
                && let Some(item) = item_of(r)
            {
                out.push((item, l));
            }
        }
        Cond::Not(c) => compared(c, item_of, out),
        Cond::And(a, b) | Cond::Or(a, b) => {
            compared(a, item_of, out);
            compared(b, item_of, out);
        }
        _ => {}
    }
}

/// The values a field takes from the dictionary, by its offset, for each of `fields` whose item
/// has literals that fit its storage.
pub(crate) fn values(fields: &[(Field, usize)], by_item: &BTreeMap<usize, Vec<Literal>>) -> BTreeMap<usize, Vec<Vec<u8>>> {
    let mut out = BTreeMap::new();
    for &(f, item) in fields {
        let Some(literals) = by_item.get(&item) else { continue };
        let encoded: BTreeSet<Vec<u8>> = literals.iter().filter_map(|l| bytes(f, l)).collect();
        if !encoded.is_empty() {
            out.insert(f.offset, encoded.into_iter().collect());
        }
    }
    out
}

/// A literal as a field of `f`'s kind holds it, None where it does not fit or the kind is not one
/// a literal is compared with here.
pub(crate) fn bytes(f: Field, literal: &Literal) -> Option<Vec<u8>> {
    match (f.kind, literal) {
        (Kind::Alnum { justified }, _) => {
            let text = text(literal, f.size)?;
            let mut b = ebcdic(&text);
            if b.len() > f.size {
                return None;
            }
            let pad = vec![SPACE; f.size - b.len()];
            if justified {
                b.splice(0..0, pad);
            } else {
                b.extend(pad);
            }
            Some(b)
        }
        (Kind::National, Literal::Alnum(s) | Literal::National(s)) => {
            let mut b: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
            if b.len() > f.size {
                return None;
            }
            while b.len() < f.size {
                b.extend_from_slice(&[0x00, 0x20]);
            }
            Some(b)
        }
        (Kind::Zoned { digits, scale, signed, sign }, Literal::Number(n)) => {
            let (negative, d) = scaled(n, digits, scale, signed)?;
            let mut b: Vec<u8> = d.bytes().map(|c| 0xF0 | (c - b'0')).collect();
            match sign {
                Some(s) if s.separate => {
                    let mark = if negative { MINUS } else { PLUS };
                    if s.position == SignPosition::Leading { b.insert(0, mark) } else { b.push(mark) }
                }
                _ if signed => {
                    let at = if sign.is_some_and(|s| s.position == SignPosition::Leading) { 0 } else { b.len() - 1 };
                    b[at] = (b[at] & 0x0F) | if negative { 0xD0 } else { 0xC0 };
                }
                _ => {}
            }
            (b.len() == f.size).then_some(b)
        }
        (Kind::Packed { digits, scale, signed }, Literal::Number(n)) => {
            let (negative, d) = scaled(n, digits, scale, signed)?;
            let mut nibbles: Vec<u8> = std::iter::repeat_n(0, (2 * f.size - 1).saturating_sub(d.len())).chain(d.bytes().map(|c| c - b'0')).collect();
            nibbles.push(match (signed, negative) {
                (false, _) => 0x0F,
                (true, false) => 0x0C,
                (true, true) => 0x0D,
            });
            (nibbles.len() == 2 * f.size).then(|| nibbles.chunks(2).map(|p| (p[0] << 4) | p[1]).collect())
        }
        (Kind::Binary { digits, scale, signed, .. }, Literal::Number(n)) => {
            let (negative, d) = scaled(n, digits, scale, signed)?;
            let magnitude: i128 = d.parse().ok()?;
            let value = if negative { -magnitude } else { magnitude };
            let all = value.to_be_bytes();
            (f.size <= all.len()).then(|| all[all.len() - f.size..].to_vec())
        }
        _ => None,
    }
}

/// The text an alphanumeric literal or figurative constant fills `size` characters with.
fn text(literal: &Literal, size: usize) -> Option<String> {
    let fill = |c: char| Some(std::iter::repeat_n(c, size).collect());
    match literal {
        Literal::Alnum(s) => Some(s.clone()),
        Literal::Number(s) if !s.contains(['.', '-', '+']) => Some(s.clone()),
        Literal::Figurative(Figurative::Space) => fill(' '),
        Literal::Figurative(Figurative::Zero) => fill('0'),
        Literal::All(inner) => {
            let unit = text(inner, size)?;
            (!unit.is_empty()).then(|| unit.chars().cycle().take(size).collect())
        }
        _ => None,
    }
}

/// A numeric literal's sign and digits at `scale` decimal places, zero-filled to `digits`; None
/// where it has more decimal places or digits than the item, or is negative for an unsigned one.
fn scaled(n: &str, digits: u32, scale: u32, signed: bool) -> Option<(bool, String)> {
    let negative = n.starts_with('-');
    let unsigned = n.trim_start_matches(['+', '-']);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let fraction = fraction.trim_end_matches('0');
    if negative && !signed || fraction.len() > scale as usize || !whole.bytes().chain(fraction.bytes()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut d = format!("{}{fraction:0<width$}", whole.trim_start_matches('0'), width = scale as usize);
    if d.len() > digits as usize {
        return None;
    }
    d = format!("{d:0>width$}", width = digits as usize);
    Some((negative && d.bytes().any(|c| c != b'0'), d))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt::vocab::SignClause;

    fn field(size: usize, kind: Kind) -> Field {
        Field { offset: 0, size, kind }
    }

    #[test]
    fn a_literal_is_stored_as_the_field_holds_it() {
        let number = |s: &str| Literal::Number(s.into());
        assert_eq!(bytes(field(6, Kind::Alnum { justified: false }), &Literal::Alnum("READ".into())), Some(vec![0xD9, 0xC5, 0xC1, 0xC4, SPACE, SPACE]));
        assert_eq!(bytes(field(3, Kind::Alnum { justified: false }), &Literal::Alnum("READ".into())), None);
        assert_eq!(bytes(field(3, Kind::Zoned { digits: 3, scale: 0, signed: true, sign: None }), &number("-12")), Some(vec![0xF0, 0xF1, 0xD2]));
        assert_eq!(bytes(field(4, Kind::Zoned { digits: 3, scale: 1, signed: true, sign: Some(SignClause { position: SignPosition::Leading, separate: true }) }), &number("2.5")), Some(vec![PLUS, 0xF0, 0xF2, 0xF5]));
        assert_eq!(bytes(field(3, Kind::Packed { digits: 5, scale: 2, signed: true }), &number("12.3")), Some(vec![0x01, 0x23, 0x0C]));
        assert_eq!(bytes(field(2, Kind::Binary { digits: 4, scale: 0, signed: true, native: numeric::Native::No }), &number("-2")), Some(vec![0xFF, 0xFE]));
        assert_eq!(bytes(field(2, Kind::Zoned { digits: 2, scale: 0, signed: false, sign: None }), &number("-1")), None);
        assert_eq!(bytes(field(2, Kind::Zoned { digits: 2, scale: 0, signed: false, sign: None }), &number("123")), None);
        assert_eq!(bytes(field(3, Kind::Alnum { justified: false }), &Literal::Figurative(Figurative::Space)), Some(vec![SPACE; 3]));
    }

    #[test]
    fn relations_evaluate_whens_and_condition_names_give_an_item_its_values() {
        let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. D.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  OP PIC X(6).\n       01  CODE-X PIC 9.\n           88 CODE-OK VALUE 1 THRU 3.\n       01  N PIC 99.\n       PROCEDURE DIVISION.\n           IF OP = 'READ' DISPLAY 'R' END-IF\n           EVALUATE N\n             WHEN 42 DISPLAY 'N'\n             WHEN OTHER CONTINUE\n           END-EVALUATE\n           PERFORM UNTIL 'WRITE' = OP CONTINUE END-PERFORM\n           GOBACK.\n";
        let mut programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap();
        let compiled = exec::compile(programs.remove(0), &[]).unwrap();
        let by_item = literals_by_item(&compiled);
        let named = |name: &str| by_item.iter().find(|(i, _)| compiled.layout.items[**i].name.as_deref() == Some(name)).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(named("OP"), [Literal::Alnum("READ".into()), Literal::Alnum("WRITE".into())]);
        assert_eq!(named("N"), [Literal::Number("42".into())]);
        assert_eq!(named("CODE-X"), [Literal::Number("1".into()), Literal::Number("3".into())]);
    }
}
