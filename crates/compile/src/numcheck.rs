//! NUMCHECK's facts known when compiled: what ZON(LAX) tolerates in each zoned item because of the
//! item its record redefines (assumption [`numeric::assumptions::NUMCHECK_LAX_REDEFINES`]), and the
//! tests the compiler finds always fail, which it reports with an error-level message and removes
//! (assumption [`numeric::assumptions::NUMCHECK_ALWAYS_FAILS`]; Programming Guide SC27-8714-03,
//! pp. 388-391).

use crate::collating::Sequence;
use crate::layout::{Kind, Layout, Resolved};
use numeric::Options;
use numeric::assumptions::NUMCHECK_ALWAYS_FAILS;
use rt::picture::Sym;
use rt::store::LaxRedefinition;
use rt::vocab::{Figurative, SignClause, SignPosition};
use std::collections::HashSet;
use syntax::ast::*;
use syntax::{Error, Pos, Severity};

#[derive(Clone, Debug, Default)]
pub struct NumcheckFacts {
    lax: Vec<Option<LaxRedefinition>>,
    /// Each reference, by its position, and item whose test was removed.
    removed: HashSet<(u16, u32, u32, usize)>,
}

impl NumcheckFacts {
    pub fn lax(&self, item: usize) -> Option<LaxRedefinition> {
        self.lax.get(item).copied().flatten()
    }

    pub fn removed(&self, item: usize, pos: Pos) -> bool {
        self.removed.contains(&(pos.file, pos.line, pos.col, item))
    }
}

/// NUMCHECK's facts for `program`, with an error for each test removed.
pub(crate) fn facts(program: &Program, layout: &Layout, declared: usize, options: &Options, collating: &Sequence, errors: &mut Vec<Error>) -> NumcheckFacts {
    let mut facts = NumcheckFacts { lax: lax_redefinitions(layout), removed: HashSet::new() };
    if options.numcheck.is_none() || program.oo.is_some() {
        return facts;
    }
    let mut tested = Tested { layout, options, found: Vec::new(), set: Vec::new() };
    for paragraph in &program.paragraphs {
        tested.statements(&paragraph.statements);
    }
    let lengths: Vec<usize> = program.files.iter().filter_map(|f| f.record_depending.as_ref()).filter_map(|r| tested.item(r)).collect();
    tested.set.extend(lengths);
    let constant: HashSet<usize> = crate::initcheck::never_set(program, layout, declared, &tested.set).into_iter().collect();
    for (pos, item, as_integer) in tested.found {
        let key = (pos.file, pos.line, pos.col, item);
        if !constant.contains(&item) || facts.removed.contains(&key) {
            continue;
        }
        let Some(bytes) = value_bytes(layout, item, options, collating) else { continue };
        let Some(why) = rt::store::numcheck_fault_in(options, layout.items[item].kind, &bytes, facts.lax(item), as_integer) else { continue };
        facts.removed.insert(key);
        let name = layout.items[item].name.as_deref().unwrap_or("FILLER");
        let hex = rt::digest::hex(&bytes).to_ascii_uppercase();
        errors.push(
            syntax::messages::IWC0110.at(pos, format!("NUMCHECK: {name} {why} wherever this statement reads it: its VALUE clauses give it X'{hex}' and no statement changes it, so the test is removed (see {NUMCHECK_ALWAYS_FAILS})"))
                .graded(Severity::Error),
        );
    }
    facts
}

/// ZON(LAX)'s tolerance of each item: an unsigned zoned item whose last byte is the last of a
/// signed trailing-overpunch item its level-01 or level-77 record redefines, or a zoned item, unsigned
/// or signed trailing-overpunch, starting where a numeric-edited item its record redefines starts.
/// An item in a table has none.
fn lax_redefinitions(layout: &Layout) -> Vec<Option<LaxRedefinition>> {
    let items = &layout.items;
    let root = |mut i: usize| {
        while let Some(p) = items[i].parent {
            i = p;
        }
        i
    };
    let overpunch = |sign: Option<SignClause>| matches!(sign, None | Some(SignClause { separate: false, position: SignPosition::Trailing }));
    (0..items.len())
        .map(|i| {
            let item = &items[i];
            let Kind::Zoned { signed, sign, .. } = item.kind else { return None };
            if !item.dims.is_empty() {
                return None;
            }
            let record = root(i);
            let target = items[record].redefines.as_deref()?;
            let redefined = (0..record).rev().find(|&s| {
                let s = &items[s];
                s.parent.is_none() && matches!(s.level, 1 | 77) && s.name.as_deref() == Some(target) && s.local == items[record].local && s.linkage.is_some() == items[record].linkage.is_some()
            })?;
            let base = &items[redefined];
            let from = item.offset - items[record].offset;
            match base.kind {
                Kind::Zoned { signed: true, sign: base_sign, .. } if !signed && overpunch(base_sign) && from + item.size == base.size => Some(LaxRedefinition::Signed),
                Kind::NumericEdited { edit, .. } if from == 0 && (!signed || overpunch(sign)) => {
                    let spaces = suppressed_lead(&layout.edits[edit as usize]).min(item.size);
                    (spaces > 0).then_some(LaxRedefinition::LeadingSpaces(spaces))
                }
                _ => None,
            }
        })
        .collect()
}

/// The positions from an edited PICTURE's start through its last leading Z, with the insertion
/// characters among them, which zero suppression leaves as spaces.
fn suppressed_lead(symbols: &[Sym]) -> u32 {
    let lead = symbols.iter().take_while(|s| matches!(s, Sym::Z | Sym::Insert(_)));
    lead.enumerate().filter(|(_, s)| **s == Sym::Z).map(|(k, _)| k as u32 + 1).last().unwrap_or(0)
}

/// The bytes the VALUE clauses give item `i` for the whole run, when one alphanumeric clause, the
/// item's own or a group's above it, covers every byte and no other VALUE clause reaches them. An
/// item in a table, a numeric item's own VALUE and a JUSTIFIED item's give none.
fn value_bytes(layout: &Layout, i: usize, options: &Options, collating: &Sequence) -> Option<Vec<u8>> {
    let items = &layout.items;
    let item = &items[i];
    if !item.dims.is_empty() {
        return None;
    }
    let covering = std::iter::successors(Some(i), |&j| items[j].parent).find(|&j| items[j].value.is_some())?;
    let c = &items[covering];
    if !matches!(c.kind, Kind::Group | Kind::Alnum { justified: false }) {
        return None;
    }
    let (start, end) = (item.offset, item.offset + item.size);
    let reaches = |j: usize| {
        let o = &items[j];
        let span: u32 = o.dims.iter().map(|&(stride, count)| stride * count.saturating_sub(1)).sum();
        o.local == item.local && o.offset < end && start < o.offset + o.size + span
    };
    if (0..items.len()).any(|j| j != covering && items[j].value.is_some() && reaches(j)) {
        return None;
    }
    let image = literal_image(c.value.as_ref()?, c.size as usize, options, collating)?;
    let from = (start - c.offset) as usize;
    Some(image[from..from + item.size as usize].to_vec())
}

/// An alphanumeric VALUE's bytes in an item of `len` bytes.
fn literal_image(literal: &Literal, len: usize, options: &Options, collating: &Sequence) -> Option<Vec<u8>> {
    let characters = |literal: &Literal| match literal {
        Literal::Alnum(s) => options.code_page().encode(s).ok(),
        Literal::Hex(b) => Some(b.clone()),
        Literal::Figurative(f) => Some(vec![collating.figurative(*f)]),
        _ => None,
    };
    match literal {
        Literal::Alnum(_) | Literal::Hex(_) => {
            let mut bytes = characters(literal).filter(|b| b.len() <= len)?;
            bytes.resize(len, collating.figurative(Figurative::Space));
            Some(bytes)
        }
        Literal::Figurative(f) => Some(vec![collating.figurative(*f); len]),
        Literal::All(inner) => {
            let pattern = characters(inner).filter(|p| !p.is_empty())?;
            Some(pattern.iter().copied().cycle().take(len).collect())
        }
        Literal::National(_) | Literal::Dbcs(_) | Literal::Number(_) => None,
    }
}

/// The references whose items the interpreter tests where it reads them: an arithmetic expression's
/// operands, a MOVE's sender, a numeric comparison's operands, a condition-name's conditional
/// variable and a BY CONTENT or BY VALUE argument, each a plain reference with neither subscripts
/// nor reference modification. The items a table SORT or a READ's record length sets, which the
/// INITCHECK analysis does not count, are gathered too.
struct Tested<'a> {
    layout: &'a Layout,
    options: &'a Options,
    /// Each reference tested, its item, and whether it is tested as an integer.
    found: Vec<(Pos, usize, bool)>,
    set: Vec<usize>,
}

impl Tested<'_> {
    fn item(&self, r: &Ref) -> Option<usize> {
        match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) => Some(i),
            _ => None,
        }
    }

    fn plain(&self, r: &Ref) -> Option<usize> {
        if r.refmod.is_some() || !r.subscripts.is_empty() {
            return None;
        }
        self.item(r)
    }

    fn kind(&self, r: &Ref) -> Option<Kind> {
        if r.refmod.is_some() {
            return Some(Kind::Alnum { justified: false });
        }
        self.item(r).map(|i| self.layout.items[i].kind)
    }

    fn sender(&mut self, r: &Ref, as_integer: bool) {
        if let Some(i) = self.plain(r) {
            self.found.push((r.pos, i, as_integer));
        }
    }

    fn statements(&mut self, stmts: &[Stmt]) {
        stmts.iter().for_each(|s| self.statement(s));
    }

    fn opt(&mut self, stmts: &Option<Vec<Stmt>>) {
        self.statements(stmts.as_deref().unwrap_or_default());
    }

    fn handlers(&mut self, h: &Handlers) {
        self.opt(&h.on);
        self.opt(&h.not_on);
    }

    fn size_error(&mut self, se: &Option<SizeError>) {
        if let Some(se) = se {
            self.statements(&se.on);
            self.statements(&se.not_on);
        }
    }

    fn statement(&mut self, s: &Stmt) {
        match s {
            Stmt::Move { from: Operand::Ref(r), to, .. } => self.moved(r, to),
            Stmt::Move { .. } => {}
            Stmt::Compute { expr, size_error, .. } => {
                self.arithmetic(expr);
                self.size_error(size_error);
            }
            Stmt::Arith(a) => {
                a.computations.iter().for_each(|(_, e)| self.arithmetic(e));
                if let Some((_, x, y)) = &a.remainder {
                    self.arithmetic(x);
                    self.arithmetic(y);
                }
                self.size_error(&a.size_error);
            }
            Stmt::If { cond, then, otherwise, .. } => {
                self.cond(cond);
                self.statements(then);
                self.statements(otherwise);
            }
            Stmt::Evaluate { subjects, whens, other, .. } => {
                for subject in subjects {
                    if let Subject::Cond(c) = subject {
                        self.cond(c);
                    }
                }
                for object in whens.iter().flat_map(|w| &w.alternatives).flatten() {
                    if let Object::Cond(c) = object {
                        self.cond(c);
                    }
                }
                whens.iter().for_each(|w| self.statements(&w.body));
                self.statements(other);
            }
            Stmt::PerformInline { body, repeat, .. } => {
                self.repeat(repeat);
                self.statements(body);
            }
            Stmt::PerformProc { repeat, .. } => self.repeat(repeat),
            Stmt::Read(r) => {
                self.handlers(&r.at_end);
                self.handlers(&r.invalid);
            }
            Stmt::Write { invalid, end_of_page, .. } => {
                self.handlers(invalid);
                self.handlers(end_of_page);
            }
            Stmt::Rewrite { invalid, .. } | Stmt::Delete { invalid, .. } | Stmt::Start { invalid, .. } => self.handlers(invalid),
            Stmt::Call(c) => {
                for arg in &c.using {
                    if let (ArgMode::Content | ArgMode::Value, Some(Operand::Ref(r))) = (arg.mode, &arg.value) {
                        self.sender(r, false);
                    }
                }
                self.opt(&c.on_exception);
                self.opt(&c.not_on_exception);
            }
            Stmt::Invoke(i) => {
                self.opt(&i.on_exception);
                self.opt(&i.not_on_exception);
            }
            Stmt::String(st) => {
                self.opt(&st.on_overflow);
                self.opt(&st.not_on_overflow);
            }
            Stmt::Unstring(u) => {
                self.opt(&u.on_overflow);
                self.opt(&u.not_on_overflow);
            }
            Stmt::Search(se) => {
                self.opt(&se.at_end);
                se.whens.iter().for_each(|(_, body)| self.statements(body));
            }
            Stmt::XmlParse(x) => {
                self.opt(&x.on_exception);
                self.opt(&x.not_on_exception);
            }
            Stmt::XmlGenerate(x) => {
                self.opt(&x.on_exception);
                self.opt(&x.not_on_exception);
            }
            Stmt::JsonParse(j) => {
                self.opt(&j.on_exception);
                self.opt(&j.not_on_exception);
            }
            Stmt::JsonGenerate(g) => {
                self.opt(&g.on_exception);
                self.opt(&g.not_on_exception);
            }
            Stmt::Sorting(so) => match &**so {
                Sorting::Sort(sort) => {
                    let table = self.item(&sort.subject);
                    self.set.extend(table);
                }
                Sorting::Return { at_end, .. } => self.handlers(at_end),
                Sorting::Release { .. } => {}
            },
            Stmt::Display { .. }
            | Stmt::Open { .. }
            | Stmt::Close { .. }
            | Stmt::DeleteFile { .. }
            | Stmt::Initialize { .. }
            | Stmt::GoTo { .. }
            | Stmt::GoToDepending { .. }
            | Stmt::Alter { .. }
            | Stmt::Entry { .. }
            | Stmt::Goback { .. }
            | Stmt::ExitProgram { .. }
            | Stmt::ExitMethod { .. }
            | Stmt::StopRun { .. }
            | Stmt::Cancel { .. }
            | Stmt::Set { .. }
            | Stmt::Accept { .. }
            | Stmt::Inspect(_)
            | Stmt::NextSentence
            | Stmt::SentenceEnd
            | Stmt::Exec(_)
            | Stmt::Report(_)
            | Stmt::Continue { .. }
            | Stmt::Exit { .. }
            | Stmt::Corresponding(_) => {}
        }
    }

    /// A MOVE's sender is tested for each receiver as the interpreter's `move_source` tests it: an
    /// alphanumeric sender as an integer for a numeric receiver, and under ZON(LAX) a zoned sender
    /// not for a zoned, alphanumeric or group receiver.
    fn moved(&mut self, from: &Ref, to: &[Ref]) {
        let Some(sender) = self.plain(from).map(|i| self.layout.items[i].kind) else { return };
        let lax = self.options.numcheck.and_then(|c| c.zon).is_some_and(|z| z.lax);
        for receiver in to {
            let Some(kind) = self.kind(receiver) else { continue };
            let numeric = matches!(kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::NumericEdited { .. });
            let spared = lax && matches!(sender, Kind::Zoned { .. }) && matches!(kind, Kind::Zoned { .. } | Kind::Alnum { .. } | Kind::Group);
            if !spared {
                self.sender(from, numeric && matches!(sender, Kind::Alnum { .. } | Kind::Group));
            }
        }
    }

    fn repeat(&mut self, repeat: &Loop) {
        match repeat {
            Loop::Once | Loop::Forever => {}
            Loop::Times(e) => self.arithmetic(e),
            Loop::Until { cond, .. } => self.cond(cond),
            Loop::Varying { varying, after, .. } => {
                for v in std::iter::once(&**varying).chain(after) {
                    self.cond(&v.until);
                }
            }
        }
    }

    fn arithmetic(&mut self, e: &Expr) {
        match e {
            Expr::Operand(Operand::Ref(r)) => self.sender(r, false),
            Expr::Operand(_) => {}
            Expr::Neg(inner) => self.arithmetic(inner),
            Expr::Bin(a, _, b) => {
                self.arithmetic(a);
                self.arithmetic(b);
            }
        }
    }

    fn cond(&mut self, c: &Cond) {
        match c {
            Cond::Rel(a, _, b) => {
                self.compared(a, b);
                self.compared(b, a);
            }
            Cond::Name(r) => {
                if r.refmod.is_none()
                    && r.subscripts.is_empty()
                    && let Ok(Resolved::Condition(k)) = self.layout.resolve(&r.name, &r.qualifiers, r.pos)
                {
                    self.found.push((r.pos, self.layout.conditions[k].item, false));
                }
            }
            Cond::Not(inner) => self.cond(inner),
            Cond::And(a, b) | Cond::Or(a, b) => {
                self.cond(a);
                self.cond(b);
            }
            Cond::Class(..) | Cond::NameOrRel { .. } => {}
        }
    }

    /// One side of a relation: an arithmetic expression's operands are tested, and a numeric item
    /// compared with a number, ZERO, an arithmetic expression or another numeric item. Where zones
    /// are compared, as some zoned items then are by their bytes, none is.
    fn compared(&mut self, e: &Expr, other: &Expr) {
        if self.options.zones_compared() {
            return;
        }
        let numeric = |kind: Kind| matches!(kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_));
        match e {
            Expr::Bin(..) | Expr::Neg(_) => self.arithmetic(e),
            Expr::Operand(Operand::Ref(r)) => {
                let against_number = match other {
                    Expr::Operand(Operand::Literal(Literal::Number(_) | Literal::Figurative(Figurative::Zero))) | Expr::Bin(..) | Expr::Neg(_) => true,
                    Expr::Operand(Operand::Ref(o)) => self.kind(o).is_some_and(numeric),
                    Expr::Operand(_) => false,
                };
                if against_number && self.kind(r).is_some_and(numeric) {
                    self.sender(r, false);
                }
            }
            Expr::Operand(_) => {}
        }
    }
}
