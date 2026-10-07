//! INITCHECK: a warning for each use of a WORKING-STORAGE or LOCAL-STORAGE item that no path to its
//! statement sets, or under INITCHECK(STRICT) that some path to it leaves unset (Programming Guide
//! SC27-8714-03, pp. 373-374). A forward analysis over the procedure's control flow, iterated to a
//! fixed point: "may be set" joins paths by union for LAX, "must be set" by intersection for STRICT.
//! What it follows and what sets an item are assumption
//! [`numeric::assumptions::INITCHECK_ANALYSIS`]; the message is
//! [`numeric::assumptions::INITCHECK_MESSAGE`].
//!
//! An out-of-line PERFORM is a call of its range: each range is summarised by what every path from
//! its first paragraph to the end of its last sets, starting from nothing; the state each PERFORM
//! passes is joined at the range's entry; and the PERFORM continues with its own state plus the
//! summary, so a range performed from two places does not carry one's items back to the other.

use crate::layout::{Kind, Layout, Resolved};
use numeric::Initcheck;
use numeric::assumptions::INITCHECK_ANALYSIS;
use std::collections::{BTreeSet, HashMap};
use syntax::ast::*;
use syntax::{Error, Pos};

/// The warnings INITCHECK gives `program`, whose first `declared` WORKING-STORAGE entries are its
/// own: those after them are special registers the compiler added.
pub(crate) fn check(program: &Program, layout: &Layout, declared: usize, mode: Initcheck) -> Vec<Error> {
    let mut a = Analysis::new(program, layout, declared, mode == Initcheck::Strict);
    a.scan();
    a.summarise();
    a.propagate();
    a.warnings()
}

/// The elementary items INITCHECK analyses that no statement sets and no CALL, INVOKE or EXEC
/// statement reaches through an address, nor any of `also_set` shares storage with: each holds
/// what its VALUE clauses give it for the whole run.
pub(crate) fn never_set(program: &Program, layout: &Layout, declared: usize, also_set: &[usize]) -> Vec<usize> {
    let mut a = Analysis::new(program, layout, declared, false);
    a.scan();
    for &i in also_set {
        let set = a.sets(i).clone();
        a.written.union(&set);
    }
    (0..a.leaves.len() as u32).filter(|&b| !a.written.contains(b) && !a.address_taken.contains(b)).map(|b| a.leaves[b as usize]).collect()
}

/// One bit per elementary item analysed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Bits(Vec<u64>);

impl Bits {
    fn new(bits: usize) -> Self {
        Bits(vec![0; bits.div_ceil(64)])
    }

    fn insert(&mut self, b: u32) {
        self.0[b as usize / 64] |= 1 << (b % 64);
    }

    fn contains(&self, b: u32) -> bool {
        self.0[b as usize / 64] & (1 << (b % 64)) != 0
    }

    fn union(&mut self, other: &Bits) {
        self.0.iter_mut().zip(&other.0).for_each(|(a, b)| *a |= b);
    }

    fn intersect(&mut self, other: &Bits) {
        self.0.iter_mut().zip(&other.0).for_each(|(a, b)| *a &= b);
    }
}

/// The items set where control reaches a point, or None where no path reaches it.
type State = Option<Bits>;

/// Joins `s` into `target`, by intersection when `strict`; whether `target` changed.
fn join_into(strict: bool, target: &mut State, s: &State) -> bool {
    let Some(s) = s else { return false };
    match target {
        None => {
            *target = Some(s.clone());
            true
        }
        Some(t) => {
            let before = t.clone();
            if strict { t.intersect(s) } else { t.union(s) }
            *t != before
        }
    }
}

type At = (u16, u32, u32);

/// The first and last paragraph a procedure-name covers.
type Covered = (usize, usize);

fn at(pos: Pos) -> At {
    (pos.file, pos.line, pos.col)
}

/// The program's own run, from the first paragraph after the DECLARATIVES.
const MAIN: usize = 0;

struct Analysis<'p> {
    program: &'p Program,
    layout: &'p Layout,
    strict: bool,
    /// Whether each layout item is analysed: WORKING-STORAGE the program declares, or
    /// LOCAL-STORAGE, outside an EXTERNAL or GLOBAL record and the records of the members ironwork
    /// supplies for CICS, SQL and DL/I, whose constants IBM's copies give VALUE clauses that
    /// ironwork's leave out and whose interface blocks the translators set.
    tracked: Vec<bool>,
    /// The bit of each named elementary item analysed.
    bit: Vec<Option<u32>>,
    /// The layout item of each bit.
    leaves: Vec<usize>,
    /// For each record analysed, the storage it shares through a level-01 REDEFINES, as an index
    /// into `shared`, which holds the bits each such storage holds.
    storage: HashMap<usize, usize>,
    shared: Vec<Vec<u32>>,
    /// What setting each item sets, and the bits a use of it reads, as they are needed.
    sets: Vec<Option<Bits>>,
    reads: Vec<Option<Vec<u32>>>,
    resolved: HashMap<(At, &'p str, &'p [String]), Option<usize>>,
    procedures: HashMap<(&'p str, Option<&'p str>), Option<Covered>>,
    refmodded: Vec<bool>,
    addressed: Vec<bool>,
    /// The bits of each record that holds an item whose address is taken: any CALL may set them.
    address_taken: Bits,
    /// What the VALUE clauses set, with every item referenced with reference modification.
    initial: Bits,
    /// What any statement sets, gathered by the scan.
    written: Bits,
    /// Each range performed, as its first paragraph and the paragraph whose end returns; MAIN's
    /// has none.
    ranges: Vec<(usize, Option<usize>)>,
    range_ids: HashMap<(usize, Option<usize>), usize>,
    /// For each paragraph whose GO TO an ALTER changes, the paragraphs it may go to instead.
    alters: Vec<Vec<usize>>,
    /// The USE FOR DEBUGGING ranges that may run as control enters each paragraph, and the USE
    /// AFTER EXCEPTION/ERROR ranges that may run after a statement on each file.
    debugging: Vec<Vec<usize>>,
    errors: Vec<Vec<usize>>,
    /// What each range's paths to its end set, from nothing; None when none returns.
    summaries: Vec<State>,
    /// For each statement and item it uses, whether each bit the use reads is set: on some path in
    /// some range under LAX, on every path in every range under STRICT.
    seen: HashMap<(At, usize), Vec<bool>>,
}

impl<'p> Analysis<'p> {
    fn new(program: &'p Program, layout: &'p Layout, declared: usize, strict: bool) -> Self {
        let items = &layout.items;
        let own = program.working_storage[..declared.min(program.working_storage.len())].iter().filter(|e| e.level != 88);
        let supplied = |e: &DataEntry| program.sources.get(usize::from(e.pos.file)).is_some_and(|s| s.starts_with("(system member"));
        let unanalysed: Vec<usize> = own.clone().enumerate().filter(|(_, e)| e.external || e.global || supplied(e)).map(|(i, _)| i).collect();
        let own = own.count();
        let root = |mut i: usize| {
            while let Some(p) = items[i].parent {
                i = p;
            }
            i
        };
        let tracked: Vec<bool> = (0..items.len()).map(|i| (i < own || items[i].local) && !unanalysed.contains(&root(i))).collect();
        let mut bit = vec![None; items.len()];
        let mut leaves = Vec::new();
        for (i, it) in items.iter().enumerate() {
            if tracked[i] && it.kind != Kind::Group && it.level != 66 && it.name.is_some() {
                bit[i] = Some(leaves.len() as u32);
                leaves.push(i);
            }
        }
        let width = leaves.len();
        let mut a = Analysis {
            program,
            layout,
            strict,
            tracked,
            bit,
            leaves,
            storage: HashMap::new(),
            shared: Vec::new(),
            sets: vec![None; items.len()],
            reads: vec![None; items.len()],
            resolved: HashMap::new(),
            procedures: HashMap::new(),
            refmodded: vec![false; items.len()],
            addressed: vec![false; items.len()],
            address_taken: Bits::new(width),
            initial: Bits::new(width),
            written: Bits::new(width),
            ranges: Vec::new(),
            range_ids: HashMap::new(),
            alters: vec![Vec::new(); program.paragraphs.len()],
            debugging: vec![Vec::new(); program.paragraphs.len()],
            errors: vec![Vec::new(); program.files.len()],
            summaries: Vec::new(),
            seen: HashMap::new(),
        };
        a.share_storage();
        a.range(program.report_writer.procedure_start, None);
        a.declaratives();
        a
    }

    /// Where the DECLARATIVES may run: a debugging section as control enters each procedure it
    /// names, or every procedure for ALL PROCEDURES, and an EXCEPTION/ERROR section after each
    /// statement on a file it names, or on any file for an open mode.
    fn declaratives(&mut self) {
        let program = self.program;
        let procedures = program.report_writer.procedure_start..program.paragraphs.len();
        for u in &program.declaratives.debugging {
            let r = self.range(u.section, Some(crate::section_end(program, u.section)));
            let entered: Vec<usize> = match u.procedures.as_slice() {
                [] => procedures.clone().collect(),
                named => named.iter().filter_map(|p| self.procedure(p)).map(|(first, _)| first).collect(),
            };
            entered.into_iter().for_each(|p| self.debugging[p].push(r));
        }
        for u in &program.declaratives.errors {
            let r = self.range(u.section, Some(crate::section_end(program, u.section)));
            for (k, f) in program.files.iter().enumerate() {
                if matches!(&u.on, ErrorUse::Mode(_)) || matches!(&u.on, ErrorUse::Files(names) if names.contains(&f.name)) {
                    self.errors[k].push(r);
                }
            }
        }
    }

    /// Groups the records analysed whose storage overlaps, as a level-01 REDEFINES makes it.
    fn share_storage(&mut self) {
        let items = &self.layout.items;
        let mut records: Vec<usize> = (0..items.len()).filter(|&i| self.tracked[i] && items[i].parent.is_none()).collect();
        records.sort_by_key(|&r| (items[r].local, self.extent(r).0));
        let mut end = None;
        for r in records {
            let (start, stop) = self.extent(r);
            match end {
                Some((local, e)) if local == items[r].local && start < e => end = Some((local, stop.max(e))),
                _ => {
                    self.shared.push(Vec::new());
                    end = Some((items[r].local, stop));
                }
            }
            self.storage.insert(r, self.shared.len() - 1);
        }
        for (b, &leaf) in self.leaves.iter().enumerate() {
            if let Some(&s) = self.storage.get(&self.root(leaf)) {
                self.shared[s].push(b as u32);
            }
        }
    }

    fn root(&self, mut i: usize) -> usize {
        while let Some(p) = self.layout.items[i].parent {
            i = p;
        }
        i
    }

    /// The bytes item `i` spans from its first occurrence to the end of its last.
    fn extent(&self, i: usize) -> (u64, u64) {
        let it = &self.layout.items[i];
        let spread: u64 = it.dims.iter().map(|&(stride, count)| u64::from(stride) * u64::from(count.saturating_sub(1))).sum();
        (u64::from(it.offset), u64::from(it.offset) + u64::from(it.size) + spread)
    }

    fn path(&self, i: usize) -> Vec<usize> {
        let mut path: Vec<usize> = std::iter::successors(Some(i), |&j| self.layout.items[j].parent).collect();
        path.reverse();
        path
    }

    /// Whether setting item `x` sets elementary item `leaf`: `x` holds it, or shares its storage
    /// through REDEFINES or RENAMES. Two items of one record share storage only below siblings
    /// that overlap, which only a REDEFINES makes them do, so the elements of a table are not
    /// taken for each other.
    fn aliased(&self, x: usize, leaf: usize) -> bool {
        let overlap = |a: (u64, u64), b: (u64, u64)| a.0 < b.1 && b.0 < a.1;
        let (px, pl) = (self.path(x), self.path(leaf));
        if pl.contains(&x) {
            return true;
        }
        if !overlap(self.extent(x), self.extent(leaf)) {
            return false;
        }
        if px[0] != pl[0] {
            return true;
        }
        let k = px.iter().zip(&pl).take_while(|(a, b)| a == b).count();
        match (px.get(k), pl.get(k)) {
            (Some(&a), Some(&b)) => overlap(self.extent(a), self.extent(b)),
            _ => true,
        }
    }

    fn sets(&mut self, i: usize) -> &Bits {
        if self.sets[i].is_none() {
            let mut set = Bits::new(self.leaves.len());
            if self.tracked[i]
                && let Some(&s) = self.storage.get(&self.root(i))
            {
                for &b in &self.shared[s] {
                    if self.aliased(i, self.leaves[b as usize]) {
                        set.insert(b);
                    }
                }
            }
            self.sets[i] = Some(set);
        }
        self.sets[i].as_ref().expect("filled above")
    }

    /// The bits a use of item `i` reads, in the order of the items: its own, a group's elementary
    /// items, or for a level-66 item those of its record it renames.
    fn ensure_reads(&mut self, i: usize) {
        if self.reads[i].is_some() {
            return;
        }
        let mut out = Vec::new();
        if self.tracked[i] {
            if self.layout.items[i].level == 66 {
                let (record, (start, end)) = (self.root(i), self.extent(i));
                let shared = self.storage.get(&record).map_or(&[][..], |&s| &self.shared[s]);
                out = shared.iter().copied().filter(|&b| {
                    let leaf = self.leaves[b as usize];
                    let (s, e) = self.extent(leaf);
                    self.root(leaf) == record && s < end && start < e
                }).collect();
            } else {
                self.elementary(i, &mut out);
            }
        }
        self.reads[i] = Some(out);
    }

    fn elementary(&self, i: usize, out: &mut Vec<u32>) {
        out.extend(self.bit[i]);
        for &c in &self.layout.items[i].children {
            self.elementary(c, out);
        }
    }

    /// The item a reference names: a data item, or a condition-name's conditional variable.
    fn item(&mut self, r: &'p Ref) -> Option<usize> {
        let key = (at(r.pos), r.name.as_str(), r.qualifiers.as_slice());
        if let Some(&found) = self.resolved.get(&key) {
            return found;
        }
        let found = match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) => Some(i),
            Ok(Resolved::Condition(c)) => Some(self.layout.conditions[c].item),
            Err(_) => None,
        };
        self.resolved.insert(key, found);
        found
    }

    fn procedure(&mut self, p: &'p ProcName) -> Option<Covered> {
        let key = (p.name.as_str(), p.section.as_deref());
        if let Some(&found) = self.procedures.get(&key) {
            return found;
        }
        let found = crate::procedure(self.program, p).ok();
        self.procedures.insert(key, found);
        found
    }

    fn range(&mut self, first: usize, exit: Option<usize>) -> usize {
        if let Some(&r) = self.range_ids.get(&(first, exit)) {
            return r;
        }
        self.ranges.push((first, exit));
        self.range_ids.insert((first, exit), self.ranges.len() - 1);
        self.ranges.len() - 1
    }

    /// Records the use of item `i` at statement `pos` where `s` is set.
    fn record(&mut self, pos: Pos, i: usize, s: &Bits) {
        self.ensure_reads(i);
        let reads = self.reads[i].as_deref().unwrap_or_default();
        if reads.is_empty() {
            return;
        }
        match self.seen.get_mut(&(at(pos), i)) {
            Some(set) => {
                for (k, &b) in reads.iter().enumerate() {
                    set[k] = if self.strict { set[k] && s.contains(b) } else { set[k] || s.contains(b) };
                }
            }
            None => {
                let set = reads.iter().map(|&b| s.contains(b)).collect();
                self.seen.insert((at(pos), i), set);
            }
        }
    }

    /// One pass over every statement: the ranges PERFORM and the other statements run, what ALTER
    /// does, and which items are reference-modified or have their address taken. Then the state
    /// the program starts in, and what any CALL may set.
    fn scan(&mut self) {
        let program = self.program;
        let mut w = Walk::new(self, Mode::Scan, None);
        for (p, paragraph) in program.paragraphs.iter().enumerate() {
            w.paragraph = p;
            w.statements(&paragraph.statements, None);
        }
        for block in &program.exec_declarations {
            w.exec_addresses(block);
        }
        // The coprocessor and the translators pass these on every EXEC statement.
        for (i, it) in self.layout.items.iter().enumerate() {
            if it.parent.is_none() && matches!(it.name.as_deref(), Some("SQLCA" | "SQLDA" | "DLZDIB")) {
                self.addressed[i] = true;
            }
        }
        let width = self.leaves.len();
        let mut taken = Bits::new(width);
        let mut initial = Bits::new(width);
        for i in 0..self.layout.items.len() {
            if !self.tracked[i] {
                continue;
            }
            if self.addressed[i]
                && let Some(&s) = self.storage.get(&self.root(i))
            {
                self.shared[s].iter().for_each(|&b| taken.insert(b));
            }
            if self.layout.items[i].value.is_some() || self.refmodded[i] {
                initial.union(self.sets(i));
            }
        }
        (self.address_taken, self.initial) = (taken, initial);
        self.summaries = vec![None; self.ranges.len()];
    }

    /// Each range's summary, to a fixed point: a range is walked again when one it performs
    /// changes.
    fn summarise(&mut self) {
        let mut callers: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); self.ranges.len()];
        let mut work: BTreeSet<usize> = (0..self.ranges.len()).filter(|&r| r != MAIN).collect();
        while let Some(r) = work.pop_first() {
            let empty = Some(Bits::new(self.leaves.len()));
            let (out, calls) = Walk::run(self, Mode::Summary, r, empty);
            for (c, _) in calls {
                callers[c].insert(r);
            }
            if out != self.summaries[r] {
                self.summaries[r] = out;
                work.extend(callers[r].iter().copied());
            }
        }
    }

    /// The state at each range's entry, from the program's start, to a fixed point, recording the
    /// uses on the way.
    fn propagate(&mut self) {
        let mut entries: Vec<State> = vec![None; self.ranges.len()];
        entries[MAIN] = Some(self.initial.clone());
        let mut work = BTreeSet::from([MAIN]);
        while let Some(r) = work.pop_first() {
            let (_, calls) = Walk::run(self, Mode::Actual, r, entries[r].clone());
            for (c, s) in calls {
                if join_into(self.strict, &mut entries[c], &s) {
                    work.insert(c);
                }
            }
        }
    }

    fn warnings(&self) -> Vec<Error> {
        let items = &self.layout.items;
        let mut found: Vec<(At, usize, usize)> = self
            .seen
            .iter()
            .filter_map(|(&(at, i), set)| {
                let k = set.iter().position(|&b| !b)?;
                Some((at, i, self.leaves[self.reads[i].as_ref()?[k] as usize]))
            })
            .collect();
        found.sort_unstable();
        let name = |i: usize| items[i].name.as_deref().unwrap_or("FILLER");
        found
            .into_iter()
            .map(|((file, line, col), i, leaf)| {
                let what = if leaf == i { "it".to_owned() } else { format!("{}, which {} holds", name(leaf), name(i)) };
                let (catalogued, message) = if self.strict {
                    (syntax::messages::IWC0289, format!("INITCHECK(STRICT): {} may be used uninitialized: a path to this statement does not set {what} (see {INITCHECK_ANALYSIS})", name(i)))
                } else {
                    (syntax::messages::IWC0290, format!("INITCHECK: {} may be used uninitialized: no path to this statement sets {what} (see {INITCHECK_ANALYSIS})", name(i)))
                };
                catalogued.at(Pos { file, line, col }, message)
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Every statement once, with no state.
    Scan,
    /// A range from nothing set: what its paths to its end set.
    Summary,
    /// A range from the state at its entry: the uses, and the state each PERFORM passes.
    Actual,
}

/// What a loop repeats: an inline PERFORM's statements, or a range.
enum Body<'p> {
    Inline(&'p [Stmt]),
    Range(Option<usize>),
}

/// One walk of a range, paragraph by paragraph from its first, following fall-through and GO TO.
struct Walk<'w, 'p> {
    a: &'w mut Analysis<'p>,
    mode: Mode,
    /// The paragraph whose end returns from the range.
    exit: Option<usize>,
    /// The program's own run, where an ENTRY statement also starts it.
    main: bool,
    paragraph: usize,
    entries: Vec<State>,
    queue: BTreeSet<usize>,
    out: State,
    next_sentence: State,
    /// What EXIT PERFORM and EXIT PERFORM CYCLE carry out of each inline PERFORM, innermost last.
    loops: Vec<(State, State)>,
    /// Each range performed, with the state the PERFORM passes it.
    calls: Vec<(usize, State)>,
}

impl<'w, 'p> Walk<'w, 'p> {
    fn new(a: &'w mut Analysis<'p>, mode: Mode, exit: Option<usize>) -> Self {
        Walk { a, mode, exit, main: false, paragraph: 0, entries: Vec::new(), queue: BTreeSet::new(), out: None, next_sentence: None, loops: Vec::new(), calls: Vec::new() }
    }

    /// Walks range `r` from `entry`: the state at the end of its last paragraph, and its PERFORMs.
    fn run(a: &'w mut Analysis<'p>, mode: Mode, r: usize, entry: State) -> (State, Vec<(usize, State)>) {
        let (first, exit) = a.ranges[r];
        let program = a.program;
        let n = program.paragraphs.len();
        let mut w = Walk::new(a, mode, exit);
        w.main = r == MAIN;
        w.entries = vec![None; n];
        if first < n {
            w.entries[first] = entry;
            w.queue.insert(first);
        }
        if w.main {
            for (p, paragraph) in program.paragraphs.iter().enumerate() {
                if paragraph.statements.iter().any(|s| matches!(s, Stmt::Entry { .. })) {
                    w.queue.insert(p);
                }
            }
        }
        while let Some(p) = w.queue.pop_first() {
            w.visit(p);
        }
        (w.out, w.calls)
    }

    fn join(&self, a: State, b: State) -> State {
        match (a, b) {
            (None, s) | (s, None) => s,
            (Some(mut x), Some(y)) => {
                if self.a.strict { x.intersect(&y) } else { x.union(&y) }
                Some(x)
            }
        }
    }

    fn visit(&mut self, p: usize) {
        self.paragraph = p;
        self.next_sentence = None;
        let program = self.a.program;
        let mut st = self.entries[p].clone();
        for r in self.a.debugging[p].clone() {
            st = self.may_run(r, st);
        }
        let st = self.statements(&program.paragraphs[p].statements, st);
        let pending = self.next_sentence.take();
        let st = self.join(st, pending);
        self.end_of(p, st);
    }

    /// Control reaching the end of paragraph `p`: it returns from the range, falls through to the
    /// next paragraph, or ends the program at the end of the procedure or of the DECLARATIVES.
    fn end_of(&mut self, p: usize, st: State) {
        if st.is_none() || self.mode == Mode::Scan {
            return;
        }
        if self.exit == Some(p) {
            join_into(self.a.strict, &mut self.out, &st);
            return;
        }
        let next = p + 1;
        if next < self.a.program.paragraphs.len() && next != self.a.program.report_writer.procedure_start {
            self.go_to(next, &st);
        }
    }

    fn go_to(&mut self, p: usize, st: &State) {
        if self.mode != Mode::Scan && join_into(self.a.strict, &mut self.entries[p], st) {
            self.queue.insert(p);
        }
    }

    fn statements(&mut self, stmts: &'p [Stmt], mut st: State) -> State {
        for s in stmts {
            st = self.statement(s, st);
        }
        st
    }

    fn opt(&mut self, body: &'p Option<Vec<Stmt>>, st: State) -> State {
        self.statements(body.as_deref().unwrap_or_default(), st)
    }

    /// A statement's ON and NOT ON phrases: `on` runs from `failed` when the statement has it, and
    /// `not_on` from `ok`.
    fn either(&mut self, on: &'p Option<Vec<Stmt>>, not_on: &'p Option<Vec<Stmt>>, failed: State, ok: State) -> State {
        let ok = self.opt(not_on, ok);
        match on {
            Some(body) => {
                let failed = self.statements(body, failed);
                self.join(ok, failed)
            }
            None => ok,
        }
    }

    fn handlers(&mut self, h: &'p Handlers, st: State) -> State {
        self.either(&h.on, &h.not_on, st.clone(), st)
    }

    fn size_error(&mut self, se: Option<&'p SizeError>, failed: State, ok: State) -> State {
        let Some(se) = se else { return ok };
        let failed = self.statements(&se.on, failed);
        let ok = self.statements(&se.not_on, ok);
        self.join(ok, failed)
    }

    fn statement(&mut self, s: &'p Stmt, mut st: State) -> State {
        match s {
            Stmt::Move { from, to, pos } => {
                self.operand(from, *pos, &st);
                to.iter().for_each(|r| self.write(r, *pos, &mut st));
            }
            Stmt::Compute { targets, expr, size_error, pos } => {
                self.expr(expr, *pos, &st);
                let failed = st.clone();
                targets.iter().for_each(|t| self.write(&t.r, *pos, &mut st));
                st = self.size_error(size_error.as_ref(), failed, st);
            }
            Stmt::Arith(a) => {
                a.computations.iter().for_each(|(_, e)| self.expr(e, a.pos, &st));
                if let Some((_, x, y)) = &a.remainder {
                    self.expr(x, a.pos, &st);
                    self.expr(y, a.pos, &st);
                }
                let failed = st.clone();
                a.computations.iter().for_each(|(t, _)| self.write(&t.r, a.pos, &mut st));
                if let Some((t, ..)) = &a.remainder {
                    self.write(&t.r, a.pos, &mut st);
                }
                st = self.size_error(a.size_error.as_ref(), failed, st);
            }
            Stmt::Corresponding(_) => {}
            Stmt::If { cond, then, otherwise, pos } => {
                self.cond(cond, *pos, &st);
                let then = self.statements(then, st.clone());
                let otherwise = self.statements(otherwise, st);
                st = self.join(then, otherwise);
            }
            Stmt::Evaluate { subjects, whens, other, pos } => {
                for subject in subjects {
                    match subject {
                        Subject::Expr(e) => self.expr(e, *pos, &st),
                        Subject::Cond(c) => self.cond(c, *pos, &st),
                        Subject::Bool(_) => {}
                    }
                }
                for object in whens.iter().flat_map(|w| &w.alternatives).flatten() {
                    match object {
                        Object::Cond(c) => self.cond(c, *pos, &st),
                        Object::Value { from, thru, .. } => {
                            self.expr(from, *pos, &st);
                            thru.iter().for_each(|t| self.expr(t, *pos, &st));
                        }
                        Object::Any | Object::Bool(_) => {}
                    }
                }
                let mut out = self.statements(other, st.clone());
                for w in whens {
                    let body = self.statements(&w.body, st.clone());
                    out = self.join(out, body);
                }
                st = out;
            }
            Stmt::PerformInline { body, repeat, pos } => {
                self.loops.push((None, None));
                let out = self.looped(repeat, *pos, st, Body::Inline(body));
                let (exit, _) = self.loops.pop().unwrap_or_default();
                st = self.join(out, exit);
            }
            Stmt::PerformProc { from, thru, repeat, pos } => {
                let range = self.range_of(from, thru.as_ref());
                st = self.looped(repeat, *pos, st, Body::Range(range));
            }
            Stmt::Display { items, pos, .. } => items.iter().for_each(|o| self.operand(o, *pos, &st)),
            Stmt::Open { files, pos } => {
                for (_, name) in files {
                    self.file_status(name, *pos, &mut st);
                }
            }
            Stmt::Close { files, pos } => {
                for (name, _) in files {
                    self.file_status(name, *pos, &mut st);
                }
            }
            Stmt::Read(r) => {
                if let Some(k) = &r.key {
                    self.read(k, r.pos, &st);
                }
                self.file_status(&r.file, r.pos, &mut st);
                let failed = st.clone();
                if let Some(into) = &r.into {
                    self.write(into, r.pos, &mut st);
                }
                let mut out = self.opt(&r.at_end.not_on, st);
                out = self.opt(&r.invalid.not_on, out);
                for on in [&r.at_end.on, &r.invalid.on].into_iter().flatten() {
                    let failed = self.statements(on, failed.clone());
                    out = self.join(out, failed);
                }
                st = out;
            }
            Stmt::Write { record, from, advancing, invalid, end_of_page, pos } => {
                if let Some(op) = from {
                    self.operand(op, *pos, &st);
                }
                if let Some(Advancing::Lines { count, .. }) = advancing {
                    self.expr(count, *pos, &st);
                }
                self.record_status(record, *pos, &mut st);
                st = self.handlers(invalid, st);
                st = self.handlers(end_of_page, st);
            }
            Stmt::Rewrite { record, from, invalid, pos } => {
                if let Some(op) = from {
                    self.operand(op, *pos, &st);
                }
                self.record_status(record, *pos, &mut st);
                st = self.handlers(invalid, st);
            }
            Stmt::Delete { file, invalid, pos } => {
                self.file_status(file, *pos, &mut st);
                st = self.handlers(invalid, st);
            }
            Stmt::DeleteFile { files, pos } => {
                for file in files {
                    self.file_status(file, *pos, &mut st);
                }
            }
            Stmt::Start { file, key, invalid, pos } => {
                if let Some((_, k)) = key {
                    self.read(k, *pos, &st);
                }
                self.file_status(file, *pos, &mut st);
                st = self.handlers(invalid, st);
            }
            Stmt::Initialize { targets, pos, with } => {
                with.iter().flat_map(|w| &w.replacing).for_each(|(_, by)| self.operand(by, *pos, &st));
                targets.iter().for_each(|r| self.write(r, *pos, &mut st));
            }
            Stmt::GoTo { target, .. } => {
                let mut targets: Vec<usize> = target.iter().filter_map(|t| self.a.procedure(t)).map(|(first, _)| first).collect();
                targets.extend(self.a.alters[self.paragraph].iter().copied());
                for t in targets {
                    self.go_to(t, &st);
                }
                st = None;
            }
            Stmt::GoToDepending { targets, on, pos } => {
                self.read(on, *pos, &st);
                for t in targets {
                    if let Some((first, _)) = self.a.procedure(t) {
                        self.go_to(first, &st);
                    }
                }
            }
            Stmt::Alter { pairs, .. } => {
                if self.mode == Mode::Scan {
                    for (from, to) in pairs {
                        if let (Some((from, _)), Some((to, _))) = (self.a.procedure(from), self.a.procedure(to)) {
                            self.a.alters[from].push(to);
                        }
                    }
                }
            }
            Stmt::Entry { .. } => {
                if self.main && self.mode == Mode::Actual {
                    let initial = Some(self.a.initial.clone());
                    st = self.join(st, initial);
                }
            }
            Stmt::Goback { .. } | Stmt::ExitProgram { .. } | Stmt::ExitMethod { .. } | Stmt::StopRun { .. } => st = None,
            Stmt::Call(c) => {
                if let Operand::Ref(r) = &c.target {
                    self.read(r, c.pos, &st);
                }
                for arg in &c.using {
                    match (arg.mode, &arg.value) {
                        (ArgMode::Reference, Some(Operand::Ref(r))) => self.address(r),
                        (_, Some(op)) => self.operand(op, c.pos, &st),
                        (_, None) => {}
                    }
                }
                let failed = st.clone();
                self.call_out(&mut st);
                for arg in &c.using {
                    if let (ArgMode::Reference, Some(Operand::Ref(r))) = (arg.mode, &arg.value) {
                        self.write(r, c.pos, &mut st);
                    }
                }
                if let Some(r) = &c.returning {
                    self.write(r, c.pos, &mut st);
                }
                st = self.either(&c.on_exception, &c.not_on_exception, failed, st);
            }
            Stmt::Invoke(i) => {
                self.read(&i.target, i.pos, &st);
                i.using.iter().for_each(|op| self.operand(op, i.pos, &st));
                let failed = st.clone();
                self.call_out(&mut st);
                if let Some(r) = &i.returning {
                    self.write(r, i.pos, &mut st);
                }
                st = self.either(&i.on_exception, &i.not_on_exception, failed, st);
            }
            Stmt::Cancel { targets, pos } => targets.iter().for_each(|o| self.operand(o, *pos, &st)),
            Stmt::Set { set, pos } => match set {
                SetStmt::ConditionTrue(targets) | SetStmt::ConditionFalse(targets) => targets.iter().for_each(|r| self.write(r, *pos, &mut st)),
                SetStmt::To { targets, value } | SetStmt::Entry { targets, entry: value } => {
                    self.operand(value, *pos, &st);
                    targets.iter().for_each(|r| self.write(r, *pos, &mut st));
                }
                SetStmt::AddressOf { value, .. } => self.operand(value, *pos, &st),
                SetStmt::UpDown { targets, by, .. } => {
                    self.expr(by, *pos, &st);
                    targets.iter().for_each(|r| self.read(r, *pos, &st));
                    targets.iter().for_each(|r| self.write(r, *pos, &mut st));
                }
                SetStmt::Switches(_) => {}
            },
            Stmt::Accept { target, exception, pos, .. } => {
                let failed = st.clone();
                self.write(target, *pos, &mut st);
                st = self.either(&exception.on, &exception.not_on, failed, st);
            }
            Stmt::String(s) => {
                for (op, delimiter) in &s.sources {
                    self.operand(op, s.pos, &st);
                    if let Delimiter::By(d) = delimiter {
                        self.operand(d, s.pos, &st);
                    }
                }
                if let Some(p) = &s.pointer {
                    self.read(p, s.pos, &st);
                }
                self.write(&s.into, s.pos, &mut st);
                if let Some(p) = &s.pointer {
                    self.write(p, s.pos, &mut st);
                }
                st = self.either(&s.on_overflow, &s.not_on_overflow, st.clone(), st);
            }
            Stmt::Unstring(u) => {
                self.read(&u.source, u.pos, &st);
                u.delimiters.iter().for_each(|(_, d)| self.operand(d, u.pos, &st));
                u.pointer.iter().chain(&u.tallying).for_each(|r| self.read(r, u.pos, &st));
                for into in &u.into {
                    self.write(&into.target, u.pos, &mut st);
                    into.delimiter_in.iter().chain(&into.count_in).for_each(|r| self.write(r, u.pos, &mut st));
                }
                u.pointer.iter().chain(&u.tallying).for_each(|r| self.write(r, u.pos, &mut st));
                st = self.either(&u.on_overflow, &u.not_on_overflow, st.clone(), st);
            }
            Stmt::Inspect(i) => {
                self.operand(&i.target, i.pos, &st);
                for p in i.tallying.iter().chain(&i.replacing) {
                    p.pattern.iter().chain(&p.by).for_each(|o| self.operand(o, i.pos, &st));
                    p.bounds.iter().for_each(|b| self.operand(&b.value, i.pos, &st));
                    if let Some(c) = &p.counter {
                        self.read(c, i.pos, &st);
                    }
                }
                if let Some((from, to, bounds)) = &i.converting {
                    self.operand(from, i.pos, &st);
                    self.operand(to, i.pos, &st);
                    bounds.iter().for_each(|b| self.operand(&b.value, i.pos, &st));
                }
                i.tallying.iter().filter_map(|p| p.counter.as_ref()).for_each(|c| self.write(c, i.pos, &mut st));
                if let (Operand::Ref(r), true) = (&i.target, !i.replacing.is_empty() || i.converting.is_some()) {
                    self.write(r, i.pos, &mut st);
                }
            }
            Stmt::Search(se) => {
                if let Some(v) = &se.varying {
                    self.write(v, se.pos, &mut st);
                }
                let mut out = self.opt(&se.at_end, st.clone());
                for (cond, body) in &se.whens {
                    self.cond(cond, se.pos, &st);
                    let body = self.statements(body, st.clone());
                    out = self.join(out, body);
                }
                st = out;
            }
            Stmt::NextSentence => {
                join_into(self.a.strict, &mut self.next_sentence, &st);
                st = None;
            }
            Stmt::SentenceEnd => {
                let pending = self.next_sentence.take();
                st = self.join(st, pending);
            }
            Stmt::Exec(block) => {
                if self.mode == Mode::Scan {
                    self.exec_addresses(block);
                }
                if block.kind == ExecKind::Cics && matches!(block.command.as_str(), "RETURN" | "XCTL" | "ABEND") {
                    st = None;
                } else if !block.declarative() {
                    self.call_out(&mut st);
                }
            }
            Stmt::Report(_) | Stmt::Continue { .. } => {}
            Stmt::JsonGenerate(g) => {
                self.read(&g.from, g.pos, &st);
                if let Some(Encoding::Ccsid(op)) = &g.encoding {
                    self.operand(op, g.pos, &st);
                }
                self.write(&g.receiver, g.pos, &mut st);
                if let Some(c) = &g.count {
                    self.write(c, g.pos, &mut st);
                }
                st = self.either(&g.on_exception, &g.not_on_exception, st.clone(), st);
            }
            Stmt::JsonParse(j) => {
                self.read(&j.source, j.pos, &st);
                if let Some(Encoding::Ccsid(op)) = &j.encoding {
                    self.operand(op, j.pos, &st);
                }
                self.write(&j.into, j.pos, &mut st);
                st = self.either(&j.on_exception, &j.not_on_exception, st.clone(), st);
            }
            Stmt::XmlParse(x) => {
                self.read(&x.document, x.pos, &st);
                if let Some(op) = &x.encoding {
                    self.operand(op, x.pos, &st);
                }
                let range = self.range_of(&x.procedure, x.thru.as_ref());
                st = self.zero_or_more(st, &Body::Range(range));
                st = self.either(&x.on_exception, &x.not_on_exception, st.clone(), st);
            }
            Stmt::XmlGenerate(x) => {
                self.read(&x.from, x.pos, &st);
                for op in [&x.encoding, &x.namespace, &x.prefix].into_iter().flatten() {
                    self.operand(op, x.pos, &st);
                }
                self.write(&x.receiver, x.pos, &mut st);
                if let Some(c) = &x.count {
                    self.write(c, x.pos, &mut st);
                }
                st = self.either(&x.on_exception, &x.not_on_exception, st.clone(), st);
            }
            Stmt::Sorting(so) => match &**so {
                Sorting::Sort(sort) => {
                    for io in [&sort.input, &sort.output].into_iter().flatten() {
                        match io {
                            SortIo::Procedure { from, thru } => {
                                let range = self.range_of(from, thru.as_ref());
                                st = self.perform(range, st);
                            }
                            SortIo::Files(names) => names.iter().for_each(|n| self.file_status(n, sort.pos, &mut st)),
                        }
                    }
                }
                Sorting::Release { from, pos, .. } => {
                    if let Some(op) = from {
                        self.operand(op, *pos, &st);
                    }
                }
                Sorting::Return { into, at_end, pos, .. } => {
                    let failed = st.clone();
                    if let Some(r) = into {
                        self.write(r, *pos, &mut st);
                    }
                    st = self.either(&at_end.on, &at_end.not_on, failed, st);
                }
            },
            Stmt::Exit { kind, .. } => match kind {
                ExitKind::Plain => {}
                ExitKind::Paragraph => self.end_of(self.paragraph, st.take()),
                ExitKind::Section => self.end_of(crate::section_end(self.a.program, self.paragraph), st.take()),
                ExitKind::Perform | ExitKind::PerformCycle => {
                    if let Some(l) = self.loops.last_mut() {
                        let target = if *kind == ExitKind::Perform { &mut l.0 } else { &mut l.1 };
                        join_into(self.a.strict, target, &st);
                        st = None;
                    }
                }
            },
        }
        st
    }

    /// A PERFORM's repetitions of `body`. An UNTIL, a VARYING or a TIMES whose count is not a
    /// literal above zero may run it no times; TEST AFTER runs it at least once.
    fn looped(&mut self, repeat: &'p Loop, pos: Pos, before: State, body: Body<'p>) -> State {
        match repeat {
            Loop::Once => self.body(&body, before),
            Loop::Forever => self.at_least_once(before, &body),
            Loop::Times(e) => {
                self.expr(e, pos, &before);
                let at_least_once = matches!(e, Expr::Operand(Operand::Literal(Literal::Number(n))) if n.trim_start_matches('+').parse::<u64>().is_ok_and(|n| n > 0));
                if at_least_once { self.at_least_once(before, &body) } else { self.zero_or_more(before, &body) }
            }
            Loop::Until { cond, test_after } => {
                let out = if *test_after { self.at_least_once(before, &body) } else { self.zero_or_more(before, &body) };
                self.cond(cond, pos, &out);
                out
            }
            Loop::Varying { varying, after, test_after } => {
                let mut st = before;
                for v in std::iter::once(&**varying).chain(after) {
                    self.expr(&v.from, pos, &st);
                    self.write(&v.var, pos, &mut st);
                }
                let out = if *test_after { self.at_least_once(st, &body) } else { self.zero_or_more(st, &body) };
                for v in std::iter::once(&**varying).chain(after) {
                    self.expr(&v.by, pos, &out);
                    self.cond(&v.until, pos, &out);
                }
                out
            }
        }
    }

    fn body(&mut self, body: &Body<'p>, st: State) -> State {
        match body {
            Body::Inline(stmts) => {
                let out = self.statements(stmts, st);
                let cycle = self.loops.last_mut().and_then(|l| l.1.take());
                self.join(out, cycle)
            }
            Body::Range(r) => self.perform(*r, st),
        }
    }

    /// The state where the loop's condition is tested before each repetition and holds.
    fn zero_or_more(&mut self, before: State, body: &Body<'p>) -> State {
        let mut head = before.clone();
        loop {
            let out = self.body(body, head.clone());
            let next = self.join(before.clone(), out);
            if next == head {
                return head;
            }
            head = next;
        }
    }

    /// The state after the last repetition of a loop that runs at least once.
    fn at_least_once(&mut self, before: State, body: &Body<'p>) -> State {
        let mut head = before.clone();
        loop {
            let out = self.body(body, head.clone());
            let next = self.join(before.clone(), out.clone());
            if next == head {
                return out;
            }
            head = next;
        }
    }

    fn range_of(&mut self, from: &'p ProcName, thru: Option<&'p ProcName>) -> Option<usize> {
        let (first, last) = self.a.procedure(from)?;
        let last = match thru {
            Some(t) => self.a.procedure(t)?.1,
            None => last,
        };
        if self.mode == Mode::Scan { Some(self.a.range(first, Some(last))) } else { self.a.range_ids.get(&(first, Some(last))).copied() }
    }

    /// An out-of-line PERFORM of range `r`: the state continues with what its paths set.
    fn perform(&mut self, r: Option<usize>, st: State) -> State {
        let (Some(r), Some(mut s)) = (r, st.clone()) else { return st };
        if self.mode == Mode::Scan {
            return st;
        }
        self.calls.push((r, st));
        let summary = self.a.summaries[r].as_ref()?;
        s.union(summary);
        Some(s)
    }

    /// A CALL, INVOKE or EXEC statement, which may set every item whose address is taken.
    fn call_out(&mut self, st: &mut State) {
        if let Some(s) = st {
            s.union(&self.a.address_taken);
        }
    }

    /// EXEC SQL host variables and EXEC CICS arguments are passed by their addresses; RECEIVE MAP
    /// and SEND MAP without INTO and FROM pass the symbolic map's.
    fn exec_addresses(&mut self, block: &'p ExecBlock) {
        block.host_variables.iter().for_each(|r| self.address(r));
        for (name, arg) in &block.options {
            match arg {
                Some(ExecArg::Operand(Operand::Ref(r) | Operand::AddressOf(r))) => self.address(r),
                Some(ExecArg::Operand(Operand::Literal(Literal::Alnum(map)))) if name == "MAP" => {
                    let map = map.trim().to_ascii_uppercase();
                    for suffix in ["I", "O"] {
                        if let Ok(Resolved::Item(i)) = self.a.layout.resolve(&format!("{map}{suffix}"), &[], block.pos) {
                            self.a.addressed[i] = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Any statement on a file sets its FILE STATUS items and, for a relative file, its RELATIVE
    /// KEY.
    fn file_status(&mut self, name: &str, pos: Pos, st: &mut State) {
        let program = self.a.program;
        if let Some(k) = program.files.iter().position(|f| f.name == name) {
            let f = &program.files[k];
            f.status.iter().chain(&f.vsam_status).chain(&f.relative_key).for_each(|r| self.write(r, pos, st));
            for r in self.a.errors[k].clone() {
                *st = self.may_run(r, st.take());
            }
        }
    }

    /// A declarative range that may run here, or not.
    fn may_run(&mut self, r: usize, st: State) -> State {
        let ran = self.perform(Some(r), st.clone());
        self.join(st, ran)
    }

    fn record_status(&mut self, record: &'p Ref, pos: Pos, st: &mut State) {
        let program = self.a.program;
        let file = self.a.item(record).and_then(|i| self.a.layout.items[i].file);
        if let Some(f) = file.and_then(|k| program.files.get(usize::from(k))) {
            self.file_status(&f.name, pos, st);
        }
    }

    fn address(&mut self, r: &'p Ref) {
        if let Some(i) = self.a.item(r) {
            self.a.addressed[i] = true;
        }
    }

    /// A use of the item `r` names, at statement `pos`. A reference-modified item counts as set.
    fn read(&mut self, r: &'p Ref, pos: Pos, st: &State) {
        self.indexes(r, pos, st);
        let Some(i) = self.a.item(r) else { return };
        if r.refmod.is_some() {
            self.a.refmodded[i] = true;
        } else if let (Mode::Actual, Some(s)) = (self.mode, st) {
            self.a.record(pos, i, s);
        }
    }

    fn write(&mut self, r: &'p Ref, pos: Pos, st: &mut State) {
        self.indexes(r, pos, st);
        let Some(i) = self.a.item(r) else { return };
        if r.refmod.is_some() {
            self.a.refmodded[i] = true;
        }
        if self.mode == Mode::Scan {
            let set = self.a.sets(i).clone();
            self.a.written.union(&set);
        }
        if let Some(s) = st {
            s.union(self.a.sets(i));
        }
    }

    fn indexes(&mut self, r: &'p Ref, pos: Pos, st: &State) {
        r.subscripts.iter().for_each(|e| self.expr(e, pos, st));
        if let Some(rm) = &r.refmod {
            self.expr(&rm.start, pos, st);
            if let Some(l) = &rm.length {
                self.expr(l, pos, st);
            }
        }
    }

    fn operand(&mut self, op: &'p Operand, pos: Pos, st: &State) {
        match op {
            Operand::Ref(r) => self.read(r, pos, st),
            Operand::AddressOf(r) => self.address(r),
            Operand::Literal(_) | Operand::LengthOf(_) => {}
            Operand::Function(f) => {
                if !matches!(f.name.as_str(), "LENGTH" | "BYTE-LENGTH") {
                    f.args.iter().for_each(|e| self.expr(e, pos, st));
                }
                if let Some(rm) = &f.refmod {
                    self.expr(&rm.start, pos, st);
                    if let Some(l) = &rm.length {
                        self.expr(l, pos, st);
                    }
                }
            }
        }
    }

    fn expr(&mut self, e: &'p Expr, pos: Pos, st: &State) {
        match e {
            Expr::Operand(op) => self.operand(op, pos, st),
            Expr::Neg(inner) => self.expr(inner, pos, st),
            Expr::Bin(a, _, b) => {
                self.expr(a, pos, st);
                self.expr(b, pos, st);
            }
        }
    }

    fn cond(&mut self, c: &'p Cond, pos: Pos, st: &State) {
        match c {
            Cond::Rel(a, _, b) => {
                self.expr(a, pos, st);
                self.expr(b, pos, st);
            }
            Cond::Class(e, _) => self.expr(e, pos, st),
            Cond::Name(r) => self.read(r, pos, st),
            Cond::NameOrRel { subject, name, .. } => {
                self.expr(subject, pos, st);
                self.read(name, pos, st);
            }
            Cond::Not(inner) => self.cond(inner, pos, st),
            Cond::And(a, b) | Cond::Or(a, b) => {
                self.cond(a, pos, st);
                self.cond(b, pos, st);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The program's INITCHECK warnings under `card`, each as the line of `procedure` it is on
    /// and the item it names, as in "4 Z"; `data` follows WORKING-STORAGE SECTION, and a line of
    /// `procedure` that starts with # is a paragraph header.
    fn warned(card: &str, data: &str, procedure: &[&str]) -> Vec<String> {
        let body: String = procedure.iter().map(|l| if let Some(h) = l.strip_prefix('#') { format!("       {h}\n") } else { format!("           {l}\n") }).collect();
        let card = if card.is_empty() { String::new() } else { format!("       CBL {card}\n") };
        let source = format!("{card}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{body}");
        let first = source.lines().position(|l| l.contains("PROCEDURE DIVISION")).unwrap() as u32 + 1;
        let compiled = crate::compile(syntax::parse(&source).unwrap(), &[]).unwrap_or_else(|e| panic!("{e:?}"));
        compiled
            .diagnostics
            .iter()
            .filter(|d| d.message.starts_with("INITCHECK"))
            .map(|d| {
                let item = d.message.split(": ").nth(1).and_then(|m| m.split(' ').next()).unwrap_or_default();
                format!("{} {item}", d.pos.line - first)
            })
            .collect()
    }

    fn both(data: &str, procedure: &[&str]) -> (Vec<String>, Vec<String>) {
        (warned("INITCHECK", data, procedure), warned("INITCHECK(STRICT)", data, procedure))
    }

    const XYZ: &str = "       01  X PIC 9.\n       01  Y PIC 9.\n       01  Z PIC 9.\n";

    #[test]
    fn lax_warns_where_no_path_sets_an_item_and_strict_where_one_does_not() {
        let program = ["IF Y > 5", "  MOVE 2 TO Z", "END-IF", "DISPLAY Z", "GOBACK."];
        assert_eq!(both(XYZ, &program), (vec!["1 Y".to_owned()], vec!["1 Y".to_owned(), "4 Z".to_owned()]));
        assert_eq!(warned("IC(LAX)", XYZ, &program), ["1 Y"]);
        assert_eq!(warned("", XYZ, &program), Vec::<String>::new());
        assert_eq!(warned("NOINITCHECK", XYZ, &program), Vec::<String>::new());
    }

    #[test]
    fn the_message_names_the_item_and_a_groups_first_item_not_set() {
        let source = |card: &str| {
            format!(
                "       CBL {card}\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  G.\n           05 A PIC X.\n           05 B PIC X.\n           05 C PIC X.\n       PROCEDURE DIVISION.\n           MOVE 'A' TO A\n           DISPLAY G C\n           GOBACK.\n"
            )
        };
        let messages = |card: &str| -> Vec<(u32, String)> {
            let compiled = crate::compile(syntax::parse(&source(card)).unwrap(), &[]).unwrap();
            compiled.diagnostics.iter().map(|d| (d.pos.line, d.message.clone())).collect()
        };
        assert_eq!(
            messages("INITCHECK"),
            [
                (12, "INITCHECK: G may be used uninitialized: no path to this statement sets B, which G holds (see C224)".to_owned()),
                (12, "INITCHECK: C may be used uninitialized: no path to this statement sets it (see C224)".to_owned()),
            ]
        );
        assert_eq!(messages("INITCHECK(STRICT)")[1].1, "INITCHECK(STRICT): C may be used uninitialized: a path to this statement does not set it (see C224)");
        let compiled = crate::compile(syntax::parse(&source("INITCHECK")).unwrap(), &[]).unwrap();
        assert!(compiled.diagnostics.iter().all(|d| d.severity == syntax::Severity::Warning));
    }

    #[test]
    fn value_clauses_and_groups_set_their_items() {
        let data = "       01  V PIC 9 VALUE 1.\n       01  G VALUE SPACES.\n           05 G1 PIC X.\n           05 G2 PIC X.\n       01  H.\n           05 H1 PIC X.\n           05 H2 PIC X.\n       01  K.\n           05 K1 PIC X.\n           05 FILLER PIC X.\n           05 K2 PIC X.\n";
        let program = ["DISPLAY V G1 G", "MOVE SPACES TO H", "DISPLAY H2", "MOVE 'A' TO K1", "MOVE 'B' TO K2", "DISPLAY K", "GOBACK."];
        assert_eq!(both(data, &program), (vec![], vec![]));
        assert_eq!(warned("INITCHECK", data, &["MOVE 'A' TO H1", "DISPLAY H", "GOBACK."]), ["2 H"]);
    }

    #[test]
    fn redefines_and_renames_share_what_is_set() {
        let data = "       01  D PIC X(4).\n       01  N REDEFINES D PIC 9(4).\n       01  R.\n           05 R1 PIC X(2).\n           05 R2 PIC X(2).\n           05 R3 REDEFINES R2 PIC 99.\n       66  RR RENAMES R1.\n";
        assert_eq!(both(data, &["MOVE '1234' TO D", "DISPLAY N", "MOVE 'AB' TO R2", "DISPLAY R3", "MOVE 'CD' TO RR", "DISPLAY R1 R", "GOBACK."]), (vec![], vec![]));
        assert_eq!(warned("INITCHECK", data, &["MOVE 'AB' TO R2", "DISPLAY R1", "GOBACK."]), ["2 R1"]);
    }

    #[test]
    fn a_table_is_one_item_and_reference_modification_counts_as_set() {
        let data = "       01  T.\n           05 E PIC X OCCURS 5.\n           05 F PIC X OCCURS 5.\n       01  M PIC X(4).\n       01  I PIC 9 VALUE 2.\n";
        assert_eq!(both(data, &["MOVE 'A' TO E(1)", "DISPLAY E(I) M(1:2) M", "GOBACK."]), (vec![], vec![]));
        assert_eq!(warned("INITCHECK", data, &["MOVE 'A' TO E(1)", "DISPLAY F(1)", "GOBACK."]), ["2 F"]);
    }

    #[test]
    fn by_reference_is_no_use_and_any_call_sets_address_taken_records() {
        let data = "       01  P.\n           05 A PIC X.\n           05 B PIC X.\n       01  C PIC X.\n       01  V PIC 9(4) BINARY.\n       01  Q PIC X.\n";
        let program = ["DISPLAY B", "CALL 'SUB' USING BY REFERENCE A BY CONTENT C BY VALUE V", "DISPLAY A B Q", "GOBACK."];
        assert_eq!(warned("INITCHECK", data, &program), ["1 B", "2 C", "2 V", "3 Q"]);
        assert_eq!(warned("INITCHECK", data, &["CALL 'OTHER'", "DISPLAY B", "CALL 'SUB' USING A", "GOBACK."]), Vec::<String>::new());
    }

    #[test]
    fn linkage_file_external_and_global_items_are_not_analysed_and_local_storage_is() {
        let data = "       01  W PIC X EXTERNAL.\n       01  GL PIC X GLOBAL.\n       LOCAL-STORAGE SECTION.\n       01  LS PIC X.\n       LINKAGE SECTION.\n       01  L PIC X.\n";
        assert_eq!(warned("INITCHECK", data, &["DISPLAY W GL LS L", "GOBACK."]), ["1 LS"]);
    }

    #[test]
    fn a_loop_may_set_an_item_for_its_next_repetition() {
        let data = "       01  X PIC 9.\n       01  I PIC 9 VALUE 0.\n";
        let program = ["PERFORM UNTIL I > 2", "  IF I > 0 DISPLAY X END-IF", "  MOVE I TO X", "  ADD 1 TO I", "END-PERFORM", "DISPLAY X", "GOBACK."];
        assert_eq!(both(data, &program), (vec![], vec!["2 X".to_owned(), "6 X".to_owned()]));
        let times = ["PERFORM 2 TIMES", "  MOVE 1 TO X", "END-PERFORM", "DISPLAY X", "GOBACK."];
        assert_eq!(both(data, &times), (vec![], vec![]));
        let exits = ["PERFORM UNTIL I > 2", "  ADD 1 TO I", "  IF I = 1 EXIT PERFORM END-IF", "  MOVE 1 TO X", "END-PERFORM", "DISPLAY X", "GOBACK."];
        assert_eq!(both(data, &exits), (vec![], vec!["6 X".to_owned()]));
    }

    #[test]
    fn a_performed_paragraph_sets_items_for_the_perform_that_ran_it() {
        let program = ["PERFORM SET-X", "DISPLAY X", "PERFORM MAYBE-Y", "DISPLAY Y", "GOBACK.", "#SET-X.", "MOVE 1 TO X.", "#MAYBE-Y.", "IF X = 1 MOVE 1 TO Y END-IF."];
        assert_eq!(both(XYZ, &program), (vec![], vec!["4 Y".to_owned()]));
        let twice = ["PERFORM SHOW", "DISPLAY X", "MOVE 1 TO X", "PERFORM SHOW", "GOBACK.", "#SHOW.", "DISPLAY 'SHOW'."];
        assert_eq!(warned("INITCHECK", XYZ, &twice), ["2 X"]);
        let callers = ["PERFORM SHOW-X", "MOVE 1 TO X", "PERFORM SHOW-X", "GOBACK.", "#SHOW-X.", "DISPLAY X."];
        assert_eq!(both(XYZ, &callers), (vec![], vec!["6 X".to_owned()]));
        let thru = ["PERFORM A THRU A-EXIT", "DISPLAY X", "GOBACK.", "#A.", "IF Y = 1 GO TO A-EXIT END-IF", "MOVE 1 TO X.", "#A-EXIT.", "EXIT."];
        assert_eq!(both(XYZ, &thru), (vec!["5 Y".to_owned()], vec!["2 X".to_owned(), "5 Y".to_owned()]));
    }

    #[test]
    fn go_to_and_fall_through_carry_what_is_set() {
        let program = ["#MAIN.", "GO TO SETUP.", "#USE-IT.", "DISPLAY X", "GOBACK.", "#SETUP.", "MOVE 1 TO X", "GO TO USE-IT."];
        assert_eq!(both(XYZ, &program), (vec![], vec![]));
        let skip = ["#FIRST-PART.", "IF Y = 1 GO TO LAST-PART END-IF", "MOVE 1 TO X.", "#LAST-PART.", "DISPLAY X", "GOBACK."];
        assert_eq!(both(XYZ, &skip), (vec!["2 Y".to_owned()], vec!["2 Y".to_owned(), "5 X".to_owned()]));
        let altered = ["#MAIN.", "ALTER SWITCH TO PROCEED TO SETUP.", "#SWITCH.", "GO TO NOWHERE.", "#NOWHERE.", "GOBACK.", "#SETUP.", "MOVE 1 TO X.", "DISPLAY X", "GOBACK."];
        assert_eq!(both(XYZ, &altered), (vec![], vec![]));
        let sentence = ["IF Y = 1 NEXT SENTENCE END-IF MOVE 1 TO X.", "DISPLAY X", "GOBACK."];
        assert_eq!(warned("INITCHECK(STRICT)", XYZ, &sentence), ["1 Y", "2 X"]);
    }

    #[test]
    fn evaluate_and_condition_names() {
        let data = "       01  F PIC X.\n           88 DONE VALUE 'Y'.\n       01  S PIC 9 VALUE 1.\n       01  X PIC 9.\n";
        let program = ["IF DONE DISPLAY 'D' END-IF", "SET DONE TO TRUE", "IF DONE DISPLAY 'D' END-IF", "EVALUATE S", "WHEN 1 MOVE 1 TO X", "WHEN OTHER MOVE 2 TO X", "END-EVALUATE", "DISPLAY X", "GOBACK."];
        assert_eq!(both(data, &program), (vec!["1 F".to_owned()], vec!["1 F".to_owned()]));
        let partial = ["EVALUATE S", "WHEN 1 MOVE 1 TO X", "END-EVALUATE", "DISPLAY X", "GOBACK."];
        assert_eq!(both(data, &partial), (vec![], vec!["4 X".to_owned()]));
    }

    #[test]
    fn file_statements_set_the_file_status_and_read_into_its_record() {
        let source = |card: &str| {
            format!(
                "       CBL {card}\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT IN-FILE ASSIGN TO INDD FILE STATUS IS FS.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  IN-FILE.\n       01  IN-REC PIC X(10).\n       WORKING-STORAGE SECTION.\n       01  FS PIC XX.\n       01  REC PIC X(10).\n       PROCEDURE DIVISION.\n           OPEN INPUT IN-FILE\n           DISPLAY FS IN-REC\n           READ IN-FILE INTO REC AT END DISPLAY 'END' END-READ\n           DISPLAY REC\n           GOBACK.\n"
            )
        };
        let lines = |card: &str| -> Vec<u32> {
            let compiled = crate::compile(syntax::parse(&source(card)).unwrap(), &[]).unwrap();
            compiled.diagnostics.iter().filter(|d| d.message.starts_with("INITCHECK")).map(|d| d.pos.line).collect()
        };
        assert_eq!(lines("INITCHECK"), Vec::<u32>::new());
        assert_eq!(lines("INITCHECK(STRICT)"), [19]);
    }

    #[test]
    fn a_declarative_may_set_an_item_and_vsam_status_and_supplied_members_are_set() {
        let source = |card: &str| {
            format!(
                "       CBL {card}\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT KS ASSIGN TO KSDD ORGANIZATION IS INDEXED\n               RECORD KEY IS KS-KEY FILE STATUS IS FS VS.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  KS.\n       01  KS-REC.\n           05 KS-KEY PIC X(5).\n           05 KS-DATA PIC X(5).\n       WORKING-STORAGE SECTION.\n       COPY DFHBMSCA.\n       01  FS PIC XX.\n       01  VS.\n           05 VS-RC PIC S9(4) BINARY.\n           05 VS-FN PIC S9(4) BINARY.\n           05 VS-FB PIC S9(4) BINARY.\n       01  ERR PIC X.\n       01  NEVER PIC X.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n       KS-ERROR SECTION.\n           USE AFTER STANDARD ERROR PROCEDURE ON KS.\n       KS-ERROR-SET.\n           MOVE 'Y' TO ERR.\n       END DECLARATIVES.\n       MAIN-LINE SECTION.\n       BEGIN.\n           OPEN INPUT KS\n           DISPLAY FS VS ERR NEVER DFHPROTN\n           GOBACK.\n"
            )
        };
        let named = |card: &str| -> Vec<String> {
            let compiled = crate::compile(syntax::parse(&source(card)).unwrap_or_else(|e| panic!("{e}")), &[]).unwrap_or_else(|e| panic!("{e:?}"));
            compiled.diagnostics.iter().filter_map(|d| d.message.split(": ").nth(1)?.split(' ').next().map(str::to_owned)).collect()
        };
        assert_eq!(named("INITCHECK"), ["NEVER"]);
        assert_eq!(named("INITCHECK(STRICT)"), ["ERR", "NEVER"]);
    }
}
