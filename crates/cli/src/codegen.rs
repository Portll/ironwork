//! Rust for a load module's programs (codegen-runtime.md §14 step 6). Each program becomes the VM's
//! dispatch loop over its blocks, written out: an op or a branch the generator has a fast path for
//! runs in generated code over the activation's storage (`rt::fast`), and anything else, or a fast
//! path that answers None before it stores, runs as the VM runs it (`rt::vm::Machine`). A fast path
//! computes what the VM's own count path computes, in the same order, from the same functions.

use numeric::precision::Places;
use rt::count::{const_number, mod_places, result_places};
use rt::fixed::places_of;
use rt::lir::{ArithPlan, Argument, Base, Comparand, Compare, Cond, CondId, Count, Expr, ExprId, Func, IntExpr, Mode, Op, Operand, Place, PlaceId, Program, Subscript, Terminator};
use rt::storage::Kind;
use rt::vocab::{BinOp, RelOp};
use numeric::Numproc;
use std::fmt::Write;

const PRELUDE: &str = "use ironwork_rt::fast::*;\nuse ironwork_rt::vm::{Exit, Machine, Next, Stop};\n";
const LINTS: &str = "#![forbid(unsafe_code)]\n#![allow(clippy::all, unreachable_code, unused_labels, unused_mut, unused_variables)]\n";

/// The executable's main.rs: each program's generated code, then `main` handing the module and that
/// code to the runtime.
pub fn main_text(module: &str, programs: &[Program]) -> String {
    let mut out = format!("{LINTS}\n{PRELUDE}");
    out.push_str(&programs_text(module, programs));
    let _ = write!(out, "\nfn main() -> std::process::ExitCode {{\n    ironwork_rt::native::main({module:?}, MODULE, &NATIVES)\n}}\n");
    out
}

/// A test harness's main.rs: every module's generated code in a module of its own, and `main` running
/// the one its first argument names, with the rest of its arguments.
pub fn harness_text(modules: &[(String, Vec<Program>)]) -> String {
    let mut out = LINTS.to_owned();
    let mut arms = String::new();
    for (k, (module, programs)) in modules.iter().enumerate() {
        let _ = write!(out, "\nmod module_{k} {{\n{PRELUDE}{}}}\n", programs_text(module, programs));
        let _ = writeln!(arms, "        Some({module:?}) => ironwork_rt::native::main_with({module:?}, module_{k}::MODULE, &module_{k}::NATIVES, args.collect()),");
    }
    let _ = write!(out, "\nfn main() -> std::process::ExitCode {{\n    let mut args = std::env::args().skip(1);\n    match args.next().as_deref() {{\n{arms}        _ => {{\n            eprintln!(\"usage: harness <module.iwm> [run flags]\");\n            std::process::ExitCode::from(2)\n        }}\n    }}\n}}\n");
    out
}

/// Each of a module's programs' generated code, its `NATIVES` table and the module's bytes.
fn programs_text(module: &str, programs: &[Program]) -> String {
    let mut out = String::new();
    let mut natives = Vec::new();
    for (k, p) in programs.iter().enumerate() {
        if p.services.class.is_some() {
            natives.push("None".to_owned());
            continue;
        }
        natives.push(format!("Some(program_{k})"));
        out.push_str(&Gen { k, p, fns: String::new() }.program());
    }
    let _ = write!(out, "\npub static NATIVES: [Option<ironwork_rt::vm::Native>; {}] = [{}];\n", natives.len(), natives.join(", "));
    let _ = writeln!(out, "\npub static MODULE: &[u8] = include_bytes!({module:?});");
    out
}

struct Gen<'a> {
    k: usize,
    p: &'a Program,
    /// The fast paths' functions, written after the program's dispatch.
    fns: String,
}

/// What a program's dispatch is written from: each op's and each branch's fast path, worked out once.
struct Code<'a> {
    k: usize,
    p: &'a Program,
    entries: &'a [u32],
    ops: &'a [Vec<Option<(String, bool)>>],
    conds: &'a [Option<String>],
    /// Each block that begins a linear SEARCH a scan runs: the scan's name, the WHEN's block and
    /// the AT END block.
    scans: &'a [Option<(String, u32, u32)>],
}

impl Code<'_> {
    /// Block `b` as a labelled block expression of type Next, from op `from`: that op run as the VM
    /// runs it where `general_first`, each other by its fast path where it has one, then the
    /// terminator.
    fn block(&self, b: usize, from: usize, general_first: bool) -> String {
        let block = &self.p.blocks[b];
        let mut out = String::from("'b: {\n");
        if from == 0 && self.entries.contains(&(b as u32)) {
            let _ = writeln!(out, "                m.entered({b});");
        }
        if !block.ops.is_empty() {
            let _ = writeln!(out, "                let mut arm = None;");
        }
        for i in from..block.ops.len() {
            let general = format!("if let Some(next) = m.run_op({b}, {i}, &mut arm, floor)? {{\n                    break 'b next;\n                }}");
            match &self.ops[b][i] {
                Some((name, arms)) if !(general_first && i == from) => {
                    let taken = if *arms { "Some(size_error) => arm = Some(u8::from(size_error))," } else { "Some(_) => {}" };
                    let _ = write!(out, "                let fast = {name}(&mut m.storage());\n                match fast {{\n                    {taken}\n                    None => {{\n                        {general}\n                    }}\n                }}\n");
                }
                _ => {
                    let _ = writeln!(out, "                {general}");
                }
            }
        }
        let arm = if block.ops.is_empty() { "None" } else { "arm" };
        let end = format!("m.end({b}, {arm}, floor)?");
        match (&block.end, &self.conds[b]) {
            (Terminator::Jump(t), _) => {
                let _ = writeln!(out, "                Next::Block({t})");
            }
            (Terminator::Branch { then, otherwise, .. }, Some(name)) => {
                let _ = write!(out, "                let fast = {name}(&m.storage());\n                match fast {{\n                    Some(true) => Next::Block({then}),\n                    Some(false) => Next::Block({otherwise}),\n                    None => {end},\n                }}\n");
            }
            _ => {
                let _ = writeln!(out, "                {end}");
            }
        }
        out.push_str("            }");
        out
    }

    /// The program's fast blocks run in a loop of their own over one storage handle, from the one
    /// control enters, until control reaches another block or a fast path declines. Where an op's
    /// does, `resume` runs it as the VM runs it and its block on from there; where a branch's
    /// does, the VM ends the block.
    fn region(&self, fast: &[bool]) -> String {
        let k = self.k;
        let mut out = format!("\nfn region_{k}(m: &mut dyn Machine, mut block: u32, floor: usize) -> Result<Next, Stop> {{\n    let leave = {{\n        let mut s = m.storage();\n        loop {{\n            match block {{\n");
        let mut resume = format!("\nfn resume_{k}(m: &mut dyn Machine, block: u32, op: usize, floor: usize) -> Result<Next, Stop> {{\n    Ok(match (block, op) {{\n");
        let go = |t: u32| if fast[t as usize] { format!("block = {t}") } else { format!("break Leave::To({t})") };
        for (b, block) in self.p.blocks.iter().enumerate().filter(|&(b, _)| fast[b]) {
            let _ = writeln!(out, "                {b} => {{");
            if let Some((name, when, at_end)) = &self.scans[b] {
                let _ = write!(out, "                    if let Some(found) = {name}(&mut s) {{\n                        if found {{\n                            {};\n                        }} else {{\n                            {};\n                        }}\n                        continue;\n                    }}\n", go(*when), go(*at_end));
            }
            let select = matches!(block.end, Terminator::Select(_));
            if select {
                let _ = writeln!(out, "                    let mut arm = None;");
            }
            for (i, op) in self.ops[b].iter().enumerate() {
                let Some((name, arms)) = op else { unreachable!("a fast block's op has a fast path") };
                if select && *arms {
                    let _ = writeln!(out, "                    match {name}(&mut s) {{\n                        Some(size_error) => arm = Some(u8::from(size_error)),\n                        None => break Leave::Op({b}, {i}),\n                    }}");
                } else {
                    let _ = writeln!(out, "                    if {name}(&mut s).is_none() {{\n                        break Leave::Op({b}, {i});\n                    }}");
                }
                let _ = writeln!(resume, "        ({b}, {i}) => {},", self.block(b, i, true));
            }
            match (&block.end, &self.conds[b]) {
                (Terminator::Jump(t), _) => {
                    let _ = writeln!(out, "                    {};", go(*t));
                }
                (Terminator::Select(targets), _) => {
                    let _ = writeln!(out, "                    match arm {{");
                    for (a, t) in targets.iter().enumerate() {
                        let _ = writeln!(out, "                        Some({a}) => {},", go(*t));
                    }
                    let _ = writeln!(out, "                        _ => unreachable!(\"the arm of a fast op is 0 or 1\"),\n                    }}");
                }
                (Terminator::Branch { then, otherwise, .. }, Some(name)) => {
                    let _ = write!(out, "                    match {name}(&s) {{\n                        Some(true) => {},\n                        Some(false) => {},\n                        None => break Leave::End({b}),\n                    }}\n", go(*then), go(*otherwise));
                }
                _ => unreachable!("a fast block ends in a jump, a fast branch or a select"),
            }
            let _ = writeln!(out, "                }}");
        }
        out.push_str("                _ => unreachable!(\"block {block} is not fast\"),\n            }\n        }\n    };\n    match leave {\n        Leave::To(t) => Ok(Next::Block(t)),\n        Leave::End(b) => m.end(b, None, floor),\n        Leave::Op(b, i) => resume_");
        let _ = write!(out, "{k}(m, b, i, floor),\n    }}\n}}\n");
        resume.push_str("        _ => unreachable!(\"op {op} of block {block} is not in a fast block\"),\n    })\n}\n");
        out.push_str(&resume);
        out
    }
}

/// A place's storage as generated code reaches it: an address expression of type Option<usize>.
struct At {
    expr: String,
    len: u32,
    kind: Kind,
}

impl Gen<'_> {
    fn program(mut self) -> String {
        let (k, p) = (self.k, self.p);
        let entries: Vec<u32> = p.paragraphs.iter().map(|para| para.entry).collect();
        let ops: Vec<Vec<Option<(String, bool)>>> = p.blocks.iter().enumerate().map(|(b, block)| block.ops.iter().enumerate().map(|(i, op)| self.op(b, i, op)).collect()).collect();
        let conds: Vec<Option<String>> = p.blocks.iter().map(|block| if let Terminator::Branch { cond, .. } = block.end { self.cond_fn(cond) } else { None }).collect();
        // A block that runs whole in generated code: no paragraph begins at it, each op has a fast
        // path, and its terminator is a jump, a branch with a fast condition, or a select of two or
        // more blocks by the arm its last op's fast path gives.
        let fast: Vec<bool> = p
            .blocks
            .iter()
            .enumerate()
            .map(|(b, block)| {
                let ends = match &block.end {
                    Terminator::Jump(_) => true,
                    Terminator::Branch { .. } => conds[b].is_some(),
                    Terminator::Select(targets) => targets.len() >= 2 && matches!(ops[b].last(), Some(Some((_, true)))),
                    _ => false,
                };
                ends && !entries.contains(&(b as u32)) && ops[b].iter().all(Option::is_some)
            })
            .collect();
        let scans: Vec<Option<(String, u32, u32)>> = (0..p.blocks.len()).map(|b| if fast[b] { self.scan(b, &fast) } else { None }).collect();
        let code = Code { k, p, entries: &entries, ops: &ops, conds: &conds, scans: &scans };
        let mut out = format!("\nfn program_{k}(m: &mut dyn Machine, block: u32, floor: usize) -> Option<Result<Exit, Stop>> {{\n    if m.watched() {{\n        return None;\n    }}\n    Some(blocks_{k}(m, block, floor))\n}}\n");
        let _ = write!(out, "\nfn blocks_{k}(m: &mut dyn Machine, mut block: u32, floor: usize) -> Result<Exit, Stop> {{\n    loop {{\n        let next = match block {{\n");
        for (b, &whole) in fast.iter().enumerate() {
            if whole {
                let _ = writeln!(out, "            {b} => region_{k}(m, {b}, floor)?,");
            } else {
                let _ = writeln!(out, "            {b} => {},", code.block(b, 0, false));
            }
        }
        out.push_str("            _ => unreachable!(\"block {block} of a verified program\"),\n        };\n        match next {\n            Next::Block(b) => block = b,\n            Next::Exit(exit) => return Ok(exit),\n        }\n    }\n}\n");
        if fast.contains(&true) {
            out.push_str(&code.region(&fast));
        }
        out.push_str(&self.fns);
        out
    }

    /// A fast path for op `i` of block `b`: its function's name, and whether it gives the op's arm.
    fn op(&mut self, b: usize, i: usize, op: &Op) -> Option<(String, bool)> {
        let name = format!("op_{}_{b}_{i}", self.k);
        let (body, arms) = match op {
            Op::Arith(id) => self.arith(&self.p.plans.arith[*id as usize])?,
            Op::Step { var, by, plan, prepass } => {
                let dest = self.at(*var)?;
                stored(dest.kind, self.p.places[*var as usize].scaling)?;
                let mut body = self.locates(prepass)?;
                let (x, px) = self.count_of(*var)?;
                let (y, py) = self.expr(*by, plan.dmax, plan.dmax)?;
                let to = result_places(px, BinOp::Add, py, plan.dmax, self.p.options.options.arith)?;
                let _ = write!(body, "    let dest = {}?;\n    let next = binop({x}, {}, BinOp::Add, {y}, {}, {})?;\n    s.store(dest, {}, {}, (next, {}), false, false)\n", dest.expr, places(px), places(py), places(to), dest.len, kind(dest.kind)?, places(to));
                (body, false)
            }
            Op::SetInt { target, value } => {
                let n = self.int(value)?;
                let dest = self.at(*target)?;
                let store = match dest.kind {
                    Kind::Index => "s.set_index(dest, n)?;\n    Some(false)".to_owned(),
                    k if stored(k, self.p.places[*target as usize].scaling).is_some() => {
                        format!("s.store(dest, {}, {}, (n, Places::new(19, 0)), false, false)", dest.len, kind(k)?)
                    }
                    _ => return None,
                };
                (format!("    let n = {n};\n    let dest = {}?;\n    {store}\n", dest.expr), false)
            }
            Op::Move { from: Operand::Load(src), to, check: _, plan } if !matches!(plan, rt::lir::MovePlan::Refused(_)) => {
                let dest = self.at(*to)?;
                if !matches!(dest.kind, Kind::Binary { .. } | Kind::Zoned { .. }) || self.p.places[*to as usize].scaling != 0 {
                    return None;
                }
                let place = &self.p.places[*src as usize];
                if place.scaling != 0 || !plain(place) {
                    return None;
                }
                let from = self.at(*src)?;
                (
                    format!(
                        "    let dest = {}?;\n    let from = {}?;\n    let n = s.integer(from, {}, {})?;\n    s.store(dest, {}, {}, (n, {}), false, false)\n",
                        dest.expr,
                        from.expr,
                        from.len,
                        kind(from.kind)?,
                        dest.len,
                        kind(dest.kind)?,
                        places(places_of(from.kind))
                    ),
                    false,
                )
            }
            Op::Set { from: Operand::Const(c), to, plan } if !matches!(plan, rt::lir::MovePlan::Refused(_)) => {
                let (n, at) = const_number(&self.p.consts[*c as usize])?;
                let dest = self.at(*to)?;
                let store = match dest.kind {
                    Kind::Index if at.dec == 0 => "s.set_index(dest, n)?;".to_owned(),
                    k if stored(k, self.p.places[*to as usize].scaling).is_some() => format!("s.store(dest, {}, {}, (n, {}), false, false)?;", dest.len, kind(k)?, places(at)),
                    _ => return None,
                };
                (format!("    let n = {n}i64;\n    let dest = {}?;\n    {store}\n    Some(false)\n", dest.expr), false)
            }
            Op::SearchAll(id) => (self.search_all(&self.p.plans.search_all[*id as usize])?, true),
            _ => return None,
        };
        let _ = write!(self.fns, "\n#[inline]\nfn {name}(s: &mut Storage) -> Option<bool> {{\n{body}}}\n");
        Some((name, arms))
    }

    /// `Vm::counted`: a one-step fixed-point plan with no REMAINDER, the prepass and the step's probe
    /// located, its value evaluated, then its receiver located, read where the step is its own
    /// operand, and stored. Its function's body, and whether the op gives an arm.
    fn arith(&self, plan: &ArithPlan) -> Option<(String, bool)> {
        let [step] = plan.steps.as_slice() else { return None };
        if step.mode != Mode::Fixed || plan.remainder.is_some() {
            return None;
        }
        let target = self.at(step.target)?;
        stored(target.kind, self.p.places[step.target as usize].scaling)?;
        let mut body = self.locates(&plan.prepass)?;
        body.push_str(&self.locates(&step.probe)?);
        let own = |e: ExprId| plan.per_receiver && self.p.exprs[e as usize] == Expr::Operand(Operand::Load(step.target));
        let (shared, own) = match self.p.exprs[step.expr as usize] {
            Expr::Bin(a, op, b) if own(a) => (b, Some((op, true))),
            Expr::Bin(a, op, b) if own(b) => (a, Some((op, false))),
            _ => (step.expr, None),
        };
        let last = if own.is_some() { plan.inner_dmax } else { plan.dmax };
        let (value, pv) = self.expr(shared, last, plan.inner_dmax)?;
        let _ = write!(body, "    let value = {value};\n    let at = {}?;\n", target.expr);
        let (result, to) = match own {
            Some((op, receiver_first)) => {
                let current = (format!("s.digits(at, {}, {})?", target.len, kind(target.kind)?), places_of(target.kind));
                let value = ("value".to_owned(), pv);
                let ((x, px), (y, py)) = if receiver_first { (current, value) } else { (value, current) };
                let to = result_places(px, op, py, plan.dmax, plan.arith)?;
                (format!("binop({x}, {}, BinOp::{op:?}, {y}, {}, {})?", places(px), places(py), places(to)), to)
            }
            None => ("value".to_owned(), pv),
        };
        let _ = write!(body, "    let result = {result};\n    s.store(at, {}, {}, (result, {}), {}, {})\n", target.len, kind(target.kind)?, places(to), step.rounded, plan.handled);
        Some((body, plan.handled))
    }

    /// `Vm::search_all`: the binary search over a table of a fixed count, its index set to each
    /// occurrence tried, each key compared in turn; true, the arm the VM gives, where no occurrence
    /// matches. A fast path that declines midway has set the index, which the VM's own search, run
    /// from the start, sets again before it reads it.
    fn search_all(&self, plan: &rt::lir::SearchAllPlan) -> Option<String> {
        let Count::Fixed(count) = plan.count else { return None };
        let index = self.at(plan.index)?;
        if index.kind != Kind::Index {
            return None;
        }
        let mut keys = String::new();
        for key in &plan.keys {
            if key.how != Compare::Fixed {
                return None;
            }
            let ordering = self.ordering(&key.key, &key.value)?;
            let ordering = if key.ascending { ordering } else { format!("{ordering}.reverse()") };
            let _ = write!(keys, "        outcome = {ordering};\n        if outcome != std::cmp::Ordering::Equal {{\n            break 'keys;\n        }}\n");
        }
        Some(format!(
            "    let (mut low, mut high) = (1i64, {count}i64);\n    while low <= high {{\n        let mid = (low + high) / 2;\n        let dest = {}?;\n        s.set_index(dest, mid)?;\n        let mut outcome = std::cmp::Ordering::Equal;\n        'keys: {{\n{keys}        }}\n        match outcome {{\n            std::cmp::Ordering::Less => low = mid + 1,\n            std::cmp::Ordering::Greater => high = mid - 1,\n            std::cmp::Ordering::Equal => return Some(false),\n        }}\n    }}\n    Some(true)\n",
            index.expr
        ))
    }

    /// A linear SEARCH beginning at fast block `a` that `Storage::scan_zoned` runs: `a`, with no ops,
    /// tests an index against a table of a fixed count; its then block, with no ops, tests one
    /// unsigned zoned key of the occurrence the index names for equality with an item of the same
    /// kind no subscript moves; that block's otherwise block only adds 1 to the index and goes back
    /// to `a`. The scan's name, the WHEN's block and the AT END block.
    fn scan(&mut self, a: usize, fast: &[bool]) -> Option<(String, u32, u32)> {
        let p = self.p;
        let Terminator::Branch { cond, then: test, otherwise: at_end } = p.blocks[a].end else { return None };
        let Cond::InTable { index, count: Count::Fixed(count) } = p.conds[cond as usize] else { return None };
        let Terminator::Branch { cond: when_cond, then: when, otherwise: step } = p.blocks[test as usize].end else { return None };
        if !p.blocks[a].ops.is_empty() || !fast[test as usize] || !p.blocks[test as usize].ops.is_empty() || !fast[step as usize] || count >= i32::MAX as u32 {
            return None;
        }
        let ix = &p.places[index as usize];
        if !statik(ix) || ix.kind != Kind::Index {
            return None;
        }
        let [Op::SetInt { target, value: IntExpr::Fixed { expr, prepass, .. } }] = p.blocks[step as usize].ops.as_slice() else { return None };
        let Expr::Bin(x, BinOp::Add, y) = p.exprs[*expr as usize] else { return None };
        let (Expr::Operand(Operand::Load(q)), Expr::Operand(Operand::Const(c))) = (&p.exprs[x as usize], &p.exprs[y as usize]) else { return None };
        let one = const_number(&p.consts[*c as usize]).is_some_and(|(n, at)| n == 1 && at.dec == 0);
        if p.blocks[step as usize].end != Terminator::Jump(a as u32) || !prepass.is_empty() || !one || !self.same(*target, index) || !self.same(*q, index) {
            return None;
        }
        let Cond::Rel { a: l, op: RelOp::Eq, b: r, how: Compare::Fixed } = &p.conds[when_cond as usize] else { return None };
        let occurrence = |c: &Comparand| {
            let Comparand::Operand(Operand::Load(e)) = c else { return None };
            let place = &p.places[*e as usize];
            let [Subscript { stride, value: IntExpr::Item(q) }] = place.subscripts.as_slice() else { return None };
            let plain = place.moved.is_empty() && place.odo.is_empty() && place.refmod.is_none() && matches!(place.base, Base::Program | Base::Local | Base::ReturnCode);
            (plain && self.same(*q, index)).then_some((place, *stride))
        };
        let ((element, stride), key) = match (occurrence(l), occurrence(r)) {
            (Some(e), None) => (e, r),
            (None, Some(e)) => (e, l),
            _ => return None,
        };
        let Comparand::Operand(Operand::Load(k)) = key else { return None };
        let (key, kind) = self.zoned(key)?;
        if element.kind != kind || element.scaling != 0 || !p.places[*k as usize].subscripts.is_empty() {
            return None;
        }
        let table = element.table.map_or("None".to_owned(), |t| format!("Some(({}, {}))", t.displacement, t.extent));
        let occurrence = |n: u32| format!("s.element(Base::{:?}, {}, {}, &[({n}, {stride})], {table})", element.base, element.offset, element.len);
        let name = format!("scan_{}_{a}", self.k);
        let _ = write!(
            self.fns,
            "\n#[inline]\nfn {name}(s: &mut Storage) -> Option<bool> {{\n    let index = s.at(Base::{:?}, {}, 4)?;\n    let from = s.integer(index, 4, Kind::Index)?;\n    let first = {}?;\n    {}?;\n    let key = {}?;\n    let (ix, found) = s.scan_zoned(index, first, {stride}, from, {count}, key, {})?;\n    s.set_index(index, ix)?;\n    Some(found)\n}}\n",
            ix.base,
            ix.offset,
            occurrence(1),
            occurrence(count),
            key.expr,
            element.len
        );
        Some((name, when, at_end))
    }

    /// Whether places `a` and `b` are the same storage at a constant address, of the same kind.
    fn same(&self, a: PlaceId, b: PlaceId) -> bool {
        let (x, y) = (&self.p.places[a as usize], &self.p.places[b as usize]);
        statik(x) && statik(y) && (x.base, x.offset, x.len, x.kind) == (y.base, y.offset, y.len, y.kind)
    }

    /// Each place of `places` located, as the VM locates them before it evaluates, None for one the
    /// VM's general path must locate.
    fn locates(&self, places: &[PlaceId]) -> Option<String> {
        let mut out = String::new();
        for &q in places {
            let _ = writeln!(out, "    {}?;", self.at(q)?.expr);
        }
        Some(out)
    }

    /// `Vm::eval_number_at`: `e` as an expression of a count, its top operation at `last` places and
    /// every one below at `inner`, with the places the count is at, which code generation works out.
    fn expr(&self, e: ExprId, last: u32, inner: u32) -> Option<(String, Places)> {
        Some(match &self.p.exprs[e as usize] {
            Expr::Operand(o) => self.operand(*o)?,
            Expr::Neg(x) => {
                let (x, px) = self.expr(*x, inner, inner)?;
                (format!("negated({x})?"), px)
            }
            Expr::Bin(a, op, b) => {
                let ((x, px), (y, py)) = (self.expr(*a, inner, inner)?, self.expr(*b, inner, inner)?);
                let to = result_places(px, *op, py, last, self.p.options.options.arith)?;
                (format!("binop({x}, {}, BinOp::{op:?}, {y}, {}, {})?", places(px), places(py), places(to)), to)
            }
            Expr::Pow(..) => return None,
        })
    }

    /// `Vm::operand_number`: an item read as a count, a numeric literal, or FUNCTION MOD of two
    /// arguments `Vm::countable` takes.
    fn operand(&self, o: Operand) -> Option<(String, Places)> {
        match o {
            Operand::Load(p) => self.count_of(p),
            Operand::Const(c) => {
                let (n, places) = const_number(&self.p.consts[c as usize])?;
                Some((format!("{n}i64"), places))
            }
            Operand::Function(f) => {
                let plan = &self.p.plans.function[f as usize];
                let (Func::Mod, None, [Argument::Value(a), Argument::Value(b)]) = (plan.func, &plan.refmod, plan.args.as_slice()) else { return None };
                let ((x, pa), (y, pb)) = (self.argument(a)?, self.argument(b)?);
                Some((format!("modulo({x}, {}, {y}, {})?", places(pa), places(pb)), mod_places(pa, pb)))
            }
            _ => None,
        }
    }

    /// `Vm::comparand_number` of an argument `Vm::countable` takes.
    fn argument(&self, c: &Comparand) -> Option<(String, Places)> {
        match c {
            Comparand::Expr { expr, dmax, mode: Mode::Fixed, prepass } => {
                let located = self.locates(prepass)?.replace('\n', " ");
                let (value, at) = self.expr(*expr, *dmax, *dmax)?;
                Some((if located.is_empty() { value } else { format!("{{ {located} {value} }}") }, at))
            }
            Comparand::Operand(Operand::Load(p)) if !matches!(self.p.places[*p as usize].kind, Kind::Index) => self.count_of(*p),
            Comparand::Operand(o @ Operand::Const(_)) => self.operand(*o),
            _ => None,
        }
    }

    /// An item of a numeric kind and no PICTURE P read as a count, at the places of its kind.
    fn count_of(&self, p: PlaceId) -> Option<(String, Places)> {
        let place = &self.p.places[p as usize];
        if place.scaling != 0 || !plain(place) || !matches!(place.kind, Kind::Index | Kind::Binary { .. } | Kind::Packed { .. } | Kind::Zoned { .. }) {
            return None;
        }
        let at = self.at(p)?;
        Some((format!("s.digits({}?, {}, {})?", at.expr, at.len, kind(at.kind)?), places_of(at.kind)))
    }

    /// `Vm::int`: an integer as the VM takes one.
    fn int(&self, e: &IntExpr) -> Option<String> {
        match e {
            IntExpr::Const(n) => Some(format!("{n}i64")),
            IntExpr::Item(p) => {
                let place = &self.p.places[*p as usize];
                if place.scaling != 0 || !plain(place) {
                    return None;
                }
                let at = self.at(*p)?;
                Some(format!("s.integer({}?, {}, {})?", at.expr, at.len, kind(at.kind)?))
            }
            IntExpr::Fixed { expr, dmax, prepass } => {
                let located = self.locates(prepass)?.replace('\n', " ");
                let (value, at) = self.expr(*expr, *dmax, *dmax)?;
                (at.dec == 0).then(|| format!("{{ {located} {value} }}"))
            }
            IntExpr::Walk(_) => None,
        }
    }

    /// Where place `p` is, for a place the VM locates without a check that can abend: a direct place,
    /// or a quick one whose subscripts are literals or static integer items.
    fn at(&self, p: PlaceId) -> Option<At> {
        let place = &self.p.places[p as usize];
        if !place.moved.is_empty() || !place.odo.is_empty() || place.refmod.is_some() {
            return None;
        }
        let expr = if place.subscripts.is_empty() {
            if !plain(place) {
                return None;
            }
            format!("s.at(Base::{:?}, {}, {})", place.base, place.offset, place.len)
        } else {
            if !matches!(place.base, Base::Program | Base::Local | Base::ReturnCode) {
                return None;
            }
            let mut subscripts = Vec::new();
            for s in &place.subscripts {
                let value = match s.value {
                    IntExpr::Const(n) => n.to_string(),
                    IntExpr::Item(q) => {
                        let item = &self.p.places[q as usize];
                        if !statik(item) || item.scaling != 0 {
                            return None;
                        }
                        format!("s.integer(s.at(Base::{:?}, {}, {})?, {}, {})?", item.base, item.offset, item.len, item.len, kind(item.kind)?)
                    }
                    _ => return None,
                };
                subscripts.push(format!("({value}, {})", s.stride));
            }
            let table = place.table.map_or("None".to_owned(), |t| format!("Some(({}, {}))", t.displacement, t.extent));
            format!("s.element(Base::{:?}, {}, {}, &[{}], {table})", place.base, place.offset, place.len, subscripts.join(", "))
        };
        Some(At { expr, len: place.len, kind: place.kind })
    }

    /// A function for condition `c` where every part of it has a fast path: its name.
    fn cond_fn(&mut self, c: CondId) -> Option<String> {
        let body = self.cond(c)?;
        let name = format!("cond_{}_{c}", self.k);
        if !self.fns.contains(&format!("fn {name}(")) {
            let _ = write!(self.fns, "\n#[inline]\nfn {name}(s: &Storage) -> Option<bool> {{\n    Some({body})\n}}\n");
        }
        Some(name)
    }

    /// `Vm::cond` of the conditions with a fast path, as an expression of type bool inside a function
    /// returning Option<bool>.
    fn cond(&self, c: CondId) -> Option<String> {
        Some(match &self.p.conds[c as usize] {
            Cond::Rel { a, op, b, how: Compare::Fixed } => {
                let ordering = self.ordering(a, b)?;
                let test = match op {
                    RelOp::Eq => "is_eq",
                    RelOp::Ne => "is_ne",
                    RelOp::Lt => "is_lt",
                    RelOp::Le => "is_le",
                    RelOp::Gt => "is_gt",
                    RelOp::Ge => "is_ge",
                };
                format!("{ordering}.{test}()")
            }
            Cond::InTable { index, count: Count::Fixed(n) } => {
                let place = &self.p.places[*index as usize];
                if place.scaling != 0 || !plain(place) {
                    return None;
                }
                let at = self.at(*index)?;
                format!("(1..={n}).contains(&s.integer({}?, {}, {})?)", at.expr, at.len, kind(at.kind)?)
            }
            Cond::Not(x) => format!("!({})", self.cond(*x)?),
            Cond::And(x, y) => format!("({} && {})", self.cond(*x)?, self.cond(*y)?),
            Cond::Or(x, y) => format!("({} || {})", self.cond(*x)?, self.cond(*y)?),
            _ => return None,
        })
    }

    /// `Vm::compare` of two operands under `Compare::Fixed` where `Vm::compare_numbers` decides it, as
    /// an expression of type Ordering.
    fn ordering(&self, a: &Comparand, b: &Comparand) -> Option<String> {
        let ((x, px), (y, py)) = (self.comparand(a)?, self.comparand(b)?);
        let by_value = format!("order({x}, {}, {y}, {})", px.dec, py.dec);
        Some(match (self.zoned(a), self.zoned(b)) {
            (Some((pa, ka)), Some((pb, kb))) if ka == kb => format!("s.zoned_order({}?, {}?, {}).or_else(|| {by_value})?", pa.expr, pb.expr, pa.len),
            _ => format!("{by_value}?"),
        })
    }

    /// A comparison's operand that is an unsigned zoned item with no SIGN clause and no PICTURE P,
    /// where it is and its kind.
    fn zoned(&self, c: &Comparand) -> Option<(At, Kind)> {
        let Comparand::Operand(Operand::Load(p)) = c else { return None };
        let place = &self.p.places[*p as usize];
        if place.scaling != 0 || !plain(place) || !matches!(place.kind, Kind::Zoned { signed: false, sign: None, .. }) {
            return None;
        }
        let at = self.at(*p)?;
        Some((at, place.kind))
    }

    /// `Vm::number_of` of a comparison's operand: an item `store::read_digits` reads, other than a
    /// packed one under NUMPROC(PFD), or a numeric literal.
    fn comparand(&self, c: &Comparand) -> Option<(String, Places)> {
        match c {
            Comparand::Operand(Operand::Load(p)) => {
                let place = &self.p.places[*p as usize];
                if matches!(place.kind, Kind::Packed { .. }) && self.p.options.options.numproc == Numproc::Pfd {
                    return None;
                }
                self.count_of(*p)
            }
            Comparand::Operand(o @ Operand::Const(_)) => self.operand(*o),
            _ => None,
        }
    }
}

/// `place::plain`: a place whose `Loc` has the place's own kind.
fn plain(place: &Place) -> bool {
    matches!(place.base, Base::Program | Base::Local | Base::Linkage(_) | Base::ReturnCode)
}

/// `place::is_static`: a place whose address is its base's plus a constant.
fn statik(place: &Place) -> bool {
    matches!(place.base, Base::Program | Base::Local | Base::ReturnCode) && place.moved.is_empty() && place.subscripts.is_empty() && place.odo.is_empty() && place.refmod.is_none()
}

/// A receiver `store::count_bytes` can store into.
fn stored(kind: Kind, scaling: u32) -> Option<()> {
    (matches!(kind, Kind::Binary { .. } | Kind::Packed { .. } | Kind::Zoned { .. }) && scaling == 0).then_some(())
}

/// A kind as Rust source under `rt::fast`'s names, for the kinds fast paths read and store.
fn kind(kind: Kind) -> Option<String> {
    Some(match kind {
        Kind::Index => "Kind::Index".to_owned(),
        Kind::Binary { digits, scale, signed, native } => format!("Kind::Binary {{ digits: {digits}, scale: {scale}, signed: {signed}, native: Native::{native:?} }}"),
        Kind::Packed { digits, scale, signed } => format!("Kind::Packed {{ digits: {digits}, scale: {scale}, signed: {signed} }}"),
        Kind::Zoned { digits, scale, signed, sign } => {
            let sign = sign.map_or("None".to_owned(), |c| format!("Some(SignClause {{ position: SignPosition::{:?}, separate: {} }})", c.position, c.separate));
            format!("Kind::Zoned {{ digits: {digits}, scale: {scale}, signed: {signed}, sign: {sign} }}")
        }
        _ => return None,
    })
}

/// Places as Rust source.
fn places(p: Places) -> String {
    format!("Places::new({}, {})", p.int, p.dec)
}
