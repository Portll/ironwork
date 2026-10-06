//! Micro Focus's and GnuCOBOL's SCREEN SECTION under `--compliance extended` (docs/compliance.md
//! IWX0020, assumption C463): each entry laid out at a line and column, each field with a PICTURE
//! given an item of that PICTURE in WORKING-STORAGE, and DISPLAY and ACCEPT of a screen written
//! out as the MOVEs and positioned DISPLAYs and ACCEPT that show and read its fields.

use crate::picture::{self, Notation};
use syntax::ast::*;
use syntax::{Error, Pos};

/// A SCREEN SECTION entry laid out: where it is, and what the compiler gave it.
struct Laid {
    line: u32,
    column: u32,
    /// The item that shows a field with a PICTURE, in WORKING-STORAGE.
    item: Option<String>,
}

/// Lays out the program's screens and writes out the DISPLAYs and ACCEPTs of them; under strict,
/// refuses the SCREEN SECTION.
pub(crate) fn expand(program: &mut Program, extended: bool, errors: &mut Vec<Error>) {
    let Some(first) = program.screens.first() else { return };
    if !extended {
        errors.push(syntax::messages::IWC0298.at(first.pos, "the SCREEN SECTION: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it"));
        return;
    }
    let notation = Notation::of(&program.environment);
    let laid = layout(&program.screens, notation, &mut program.working_storage, errors);
    let screens = std::mem::take(&mut program.screens);
    let context = Context { screens: &screens, laid: &laid };
    for p in &mut program.paragraphs {
        context.rewrite(&mut p.statements, errors);
    }
    program.screens = screens;
}

/// Each entry's line and column: LINE and COLUMN as written, PLUS and MINUS from the entry before,
/// the line before when LINE is not written, column 1 when LINE is and COLUMN is not, and the
/// column after the entry before when neither is. A field's length is its VALUE's or its
/// PICTURE's characters.
fn layout(entries: &[ScreenEntry], notation: Notation, storage: &mut Vec<DataEntry>, errors: &mut Vec<Error>) -> Vec<Laid> {
    let (mut line, mut end) = (1u32, 0u32);
    let mut laid = Vec::with_capacity(entries.len());
    for (k, e) in entries.iter().enumerate() {
        let at = |place: Option<ScreenPlace>, base: u32| match place {
            Some(ScreenPlace::At(n)) => n,
            Some(ScreenPlace::Plus(n)) => base + n,
            Some(ScreenPlace::Minus(n)) => base.saturating_sub(n).max(1),
            None => base,
        };
        let new_line = at(e.line, line);
        let column = match (e.line, e.column) {
            (_, Some(c)) => at(Some(c), end),
            (Some(_), None) => 1,
            (None, None) => end + 1,
        }
        .max(1);
        let len = match (&e.value, &e.picture) {
            (Some(v), _) => literal_text(v).chars().count() as u32,
            (None, Some(p)) => match picture::analyse_with(p, notation) {
                Ok(pic) => pic.size,
                Err((message, m)) => {
                    errors.push(message.at(e.pos, m));
                    0
                }
            },
            (None, None) => 0,
        };
        let item = e.picture.as_ref().filter(|_| e.value.is_none()).map(|p| {
            let name = format!("SCREEN%{k}");
            let mut entry = crate::report::entry(1, Some(name.clone()), Some(p.clone()), None, e.pos);
            let numeric = picture::analyse_with(p, notation).is_ok_and(|pic| matches!(pic.category, picture::Category::Numeric));
            entry.value = Some(Literal::Figurative(if numeric { Figurative::Zero } else { Figurative::Space }));
            storage.push(entry);
            name
        });
        line = new_line;
        if e.value.is_some() || e.picture.is_some() {
            end = column + len - len.min(1);
        } else {
            end = column.saturating_sub(1);
        }
        laid.push(Laid { line: new_line, column, item });
    }
    laid
}

/// A literal's characters as a screen shows them.
fn literal_text(l: &Literal) -> String {
    match l {
        Literal::Alnum(s) | Literal::National(s) | Literal::Dbcs(s) | Literal::Number(s) => s.clone(),
        Literal::Hex(b) => " ".repeat(b.len()),
        Literal::All(inner) => literal_text(inner),
        Literal::Figurative(_) => " ".into(),
    }
}

struct Context<'a> {
    screens: &'a [ScreenEntry],
    laid: &'a [Laid],
}

impl Context<'_> {
    /// The entry a name gives, and the entries it holds: those after it of a higher level.
    fn screen(&self, name: &str) -> Option<std::ops::Range<usize>> {
        let k = self.screens.iter().position(|e| e.name.as_deref() == Some(name))?;
        let level = self.screens[k].level;
        let end = self.screens[k + 1..].iter().position(|e| e.level <= level).map_or(self.screens.len(), |n| k + 1 + n);
        Some(k..end)
    }

    fn rewrite(&self, statements: &mut Vec<Stmt>, errors: &mut Vec<Error>) {
        let mut k = 0;
        while k < statements.len() {
            for body in crate::oo::bodies_mut(&mut statements[k]) {
                self.rewrite(body, errors);
            }
            if let Some(written) = self.written_out(&statements[k], errors) {
                let n = written.len();
                statements.splice(k..=k, written);
                k += n;
            } else {
                k += 1;
            }
        }
    }

    /// DISPLAY or ACCEPT of a screen as the statements that show it and, for ACCEPT, read its TO
    /// and USING fields.
    fn written_out(&self, s: &Stmt, errors: &mut Vec<Error>) -> Option<Vec<Stmt>> {
        let (name, phrases, accept, pos) = match s {
            Stmt::Display { items, screen, pos, .. } => match items.as_slice() {
                [Operand::Ref(r)] if r.qualifiers.is_empty() && r.subscripts.is_empty() => (r, screen.as_deref(), None, *pos),
                _ => return None,
            },
            Stmt::Accept { target, screen, exception, pos, .. } if target.qualifiers.is_empty() => (target, screen.as_deref(), Some(exception), *pos),
            _ => return None,
        };
        let range = self.screen(&name.name)?;
        let (line_offset, column_offset) = match phrases.and_then(|p| p.at.as_ref()) {
            None => (0, 0),
            Some(ScreenAt::Combined(Operand::Literal(Literal::Number(n)))) => {
                let n: u64 = n.parse().unwrap_or(0);
                let (l, c) = if n > 9999 { (n / 1000, n % 1000) } else { (n / 100, n % 100) };
                (l.saturating_sub(1) as u32, c.saturating_sub(1) as u32)
            }
            Some(ScreenAt::LineColumn { line, column }) => {
                let number = |o: &Option<Operand>| match o {
                    Some(Operand::Literal(Literal::Number(n))) => n.parse::<u32>().ok().map(|n| n.saturating_sub(1)),
                    None => Some(0),
                    _ => None,
                };
                match (number(line), number(column)) {
                    (Some(l), Some(c)) => (l, c),
                    _ => return Some(vec![refused(name, pos, errors)]),
                }
            }
            Some(_) => return Some(vec![refused(name, pos, errors)]),
        };
        let verb = if accept.is_some() { "ACCEPT" } else { "DISPLAY" };
        errors.push(syntax::messages::IWX0020.at(pos, format!("{verb} {} on the screen (Micro Focus and GnuCOBOL; Enterprise COBOL has none): at the lines and columns its SCREEN SECTION entries give", name.name)));
        let position = |k: usize| (self.laid[k].line + line_offset, self.laid[k].column + column_offset);
        let reference = |name: &str| Ref { name: name.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
        let mut out = Vec::new();
        let mut inputs = Vec::new();
        for k in range {
            let e = &self.screens[k];
            if e.value.is_none() && e.picture.is_none() && !(e.blank_screen || e.blank_line || e.erase_eol || e.erase_eos) {
                continue;
            }
            let (line, column) = position(k);
            let at = Some(ScreenAt::LineColumn { line: Some(Operand::Literal(Literal::Number(line.to_string()))), column: Some(Operand::Literal(Literal::Number(column.to_string()))) });
            let items = match (&e.value, &self.laid[k].item) {
                (Some(v), _) => vec![Operand::Literal(v.clone())],
                (None, Some(item)) => {
                    if let Some(source) = e.from.clone().or_else(|| e.using.clone().map(Operand::Ref)) {
                        out.push(Stmt::Move { from: source, to: vec![reference(item)], pos });
                    }
                    if let (Some(target), true) = (e.to.as_ref().or(e.using.as_ref()), accept.is_some()) {
                        inputs.push(ScreenInput { field: reference(item), target: target.clone(), line, column, update: e.using.is_some() || e.from.is_some(), secure: e.secure });
                    }
                    vec![Operand::Ref(reference(item))]
                }
                (None, None) => Vec::new(),
            };
            let phrases = ScreenPhrases {
                at,
                blank_screen: e.blank_screen,
                blank_line: e.blank_line,
                erase_eol: e.erase_eol,
                erase_eos: e.erase_eos,
                secure: e.secure,
                attributes: e.attributes.clone(),
                screen: Some(name.name.clone()),
                pos: e.pos,
                ..ScreenPhrases::default()
            };
            out.push(Stmt::Display { items, upon: None, no_advancing: false, screen: Some(Box::new(phrases)), pos });
        }
        if let Some(exception) = accept {
            let phrases = ScreenPhrases { screen: Some(name.name.clone()), inputs, pos, ..ScreenPhrases::default() };
            out.push(Stmt::Accept { target: name.clone(), from: AcceptFrom::Sysin, exception: exception.clone(), screen: Some(Box::new(phrases)), pos });
        }
        Some(out)
    }
}

/// A DISPLAY or ACCEPT of a screen at a place an item gives, which the compiler does not lay out.
fn refused(name: &Ref, pos: Pos, errors: &mut Vec<Error>) -> Stmt {
    errors.push(syntax::messages::IWR0057.at(pos, format!("DISPLAY or ACCEPT of screen {} at a place an item gives is not supported yet", name.name)));
    Stmt::Display { items: Vec::new(), upon: None, no_advancing: false, screen: None, pos }
}
