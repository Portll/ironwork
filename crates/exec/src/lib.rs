//! ironwork for COBOL: WORKING-STORAGE laid out as IBM lays it out, and an interpreter that runs a
//! program against it in EBCDIC with the numeric model of `ironwork-numeric`.

pub mod cics;
pub mod codec;
pub mod edit;
pub mod files;
pub mod layout;
pub mod machine;
pub mod picture;
pub mod sql;
pub mod strings;
pub mod terminal;
pub mod tn3270;
pub mod unit;

pub use machine::{Abend, Ending};

use layout::Layout;
use numeric::Options;
use std::io::{BufRead, Write};
use syntax::ast::*;
use syntax::{Error, Pos};

pub struct Compiled {
    pub program: Program,
    pub layout: Layout,
    pub options: Options,
    pub ssrange: bool,
}

const FUNCTIONS: &[&str] = &[
    "CHAR", "ORD", "NATIONAL-OF", "LENGTH", "UPPER-CASE", "LOWER-CASE", "REVERSE", "CURRENT-DATE", "NUMVAL", "NUMVAL-C", "TRIM", "MOD", "REM",
    "INTEGER", "INTEGER-PART", "ABS", "MIN", "MAX", "INTEGER-OF-DATE", "DATE-OF-INTEGER",
];

/// Checks and lays out a parsed program. `flags` are this compiler's own, such as `-silent`.
pub fn compile(program: Program, flags: &[String]) -> Result<Compiled, Vec<Error>> {
    let mut errors = Vec::new();
    let mut options = Options::default();
    let mut ssrange = false;
    for option in &program.options {
        let upper = option.to_ascii_uppercase();
        if upper.starts_with("SSRANGE") || upper == "SSR" {
            ssrange = true;
        } else if upper == "NOSSRANGE" || upper == "NOSSR" {
            ssrange = false;
        }
        if let Err(e) = options.apply(option) {
            errors.push(Error::at(Pos::default(), format!("CBL {option}: {e}")));
        }
    }
    for flag in flags {
        if let Err(e) = options.apply_flag(flag) {
            errors.push(Error::at(Pos::default(), e.to_string()));
        }
    }
    let files: Vec<(&[DataEntry], Option<u32>)> = program.files.iter().map(|f| (f.records.as_slice(), f.record_max)).collect();
    let layout = match layout::build(&program.working_storage, &files, &program.linkage, &program.local_storage) {
        Ok(l) => l,
        Err(e) => {
            errors.push(e);
            return Err(errors.into_iter().map(|e| e.in_files(&program.sources)).collect());
        }
    };
    for item in &layout.items {
        if let Some(object) = &item.depending_on {
            match layout.resolve(&object.name, &object.qualifiers, object.pos) {
                Ok(layout::Resolved::Item(i)) if layout.items[i].kind.is_numeric() => {}
                Ok(_) => errors.push(Error::at(object.pos, format!("OCCURS DEPENDING ON {}: not a numeric data item", object.name))),
                Err(e) => errors.push(e),
            }
        }
    }
    for param in &program.using {
        let is_record = layout.linkage_roots.iter().any(|&i| layout.items[i].name.as_deref() == Some(param.name.as_str()));
        if !is_record {
            errors.push(Error::at(Pos::default(), format!("PROCEDURE DIVISION USING {}: not an 01 or 77 item of the LINKAGE SECTION", param.name)));
        }
    }
    let mut check = Check { layout: &layout, program: &program, errors: &mut errors };
    for k in 0..program.files.len() {
        check.file_keys(k);
    }
    for block in &program.exec_declarations {
        check.exec_block(block);
    }
    for p in &program.paragraphs {
        check.statements(&p.statements);
    }
    if errors.is_empty() {
        Ok(Compiled { program, layout, options, ssrange })
    } else {
        Err(errors.into_iter().map(|e| e.in_files(&program.sources)).collect())
    }
}

/// The last paragraph of the section that paragraph `i` is in, or `i` when there are no sections.
pub(crate) fn section_end(program: &Program, i: usize) -> usize {
    let paragraphs = &program.paragraphs;
    let Some(header) = (0..=i).rev().find(|&j| paragraphs[j].is_section) else { return i };
    (header + 1..paragraphs.len()).take_while(|&j| !paragraphs[j].is_section).last().unwrap_or(header)
}

/// The first and last paragraph a procedure name covers: one paragraph, or a whole section.
pub(crate) fn procedure(program: &Program, p: &ProcName) -> Result<(usize, usize), String> {
    let found: Vec<usize> = program
        .paragraphs
        .iter()
        .enumerate()
        .filter(|(_, q)| q.name == p.name && (p.section.is_none() || (q.section == p.section && !q.is_section)))
        .map(|(i, _)| i)
        .collect();
    match found.as_slice() {
        [i] if program.paragraphs[*i].is_section => Ok((*i, section_end(program, *i))),
        [i] => Ok((*i, *i)),
        [] => Err(format!("no paragraph or section named {}", p.name)),
        _ => Err(format!("{} names more than one paragraph; qualify it with OF and its section", p.name)),
    }
}

impl Compiled {
    pub fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<Ending, Abend> {
        self.run_with(files::Dds::default(), out, err)
    }

    /// Runs with the DDs that ASSIGN names map to.
    pub fn run_with(&self, dds: files::Dds, out: &mut dyn Write, err: &mut dyn Write) -> Result<Ending, Abend> {
        self.execute(unit::Library::default(), dds, None, unit::Clock::System, out, err).map(|(ending, _)| ending)
    }

    /// Runs as the first program of a run unit: CALL finds other programs in `library`, ACCEPT
    /// reads `sysin`. Returns how the run ended and RETURN-CODE.
    pub fn execute<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend> {
        self.execute_with(library, dds, sysin, clock, None, out, err)
    }

    /// Runs as [`Compiled::execute`] does, with EXEC SQL answered by `database`.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<Box<dyn sql::Database + 'w>>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend> {
        let mut run_unit = unit::RunUnit::new(library, dds, sysin, clock, out, err);
        run_unit.sql = database.map(sql::Session::new);
        let me = run_unit.add(None, &self.program, self.layout.size as usize);
        let ending = machine::Machine::activation(self, me, &mut run_unit, true).and_then(|mut m| m.run_procedure());
        let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(&self.program.id, ending.is_ok()).map(drop));
        let closed = run_unit.close_all();
        let ending = ending?;
        settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default() })?;
        closed.map_err(|m| Abend { code: "IRONWORK".into(), message: m, pos: Pos::default() })?;
        Ok((ending, run_unit.return_code()))
    }
}

impl Compiled {
    /// Runs as the first program of a CICS task. `task` says who started it, what COMMAREA it
    /// starts with, and what files and queues it has. Returns how the run ended and the task, with
    /// RETURN TRANSID and COMMAREA if it ended that way.
    pub fn execute_cics<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend> {
        self.execute_cics_with(library, dds, task, clock, None, out, err)
    }

    /// A CICS task with a database: SYNCPOINT commits, and the end of the task commits, or rolls
    /// back after an abend.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_cics_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        mut task: cics::Task,
        clock: unit::Clock,
        database: Option<Box<dyn sql::Database + 'w>>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend> {
        let mut run_unit = unit::RunUnit::new(library, dds, None, clock, out, err);
        run_unit.sql = database.map(sql::Session::new);
        let me = run_unit.add(None, &self.program, self.layout.size as usize);
        run_unit.eib = run_unit.push_temporary(&[0; cics::EIB_LEN]);
        let commarea = task.commarea.take();
        let length = commarea.as_ref().map_or(0, Vec::len);
        let commarea = commarea.map(|c| run_unit.push_temporary(&c));
        run_unit.cics = Some(task);
        let ending = machine::Machine::activation(self, me, &mut run_unit, true).and_then(|mut m| {
            m.begin_task(commarea, length);
            m.run_procedure()
        });
        let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(&self.program.id, ending.is_ok()).map(drop));
        let mut closed = run_unit.close_all();
        for (name, f) in run_unit.cics_files.drain() {
            if let Err(e) = f.close() {
                closed = closed.and(Err(format!("closing CICS file {name}: {e}")));
            }
        }
        let mut task = run_unit.cics.take().unwrap_or_default();
        if let Err(e) = task.flush_td(self.options.code_page()) {
            closed = closed.and(Err(format!("writing transient data: {e}")));
        }
        let ending = ending.map_err(|a| match a.code.strip_prefix("S0C") {
            Some(_) => Abend { message: format!("{} ({}, which CICS reports as ASRA)", a.message, a.code), code: "ASRA".into(), pos: a.pos },
            None => a,
        })?;
        settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default() })?;
        closed.map_err(|m| Abend { code: "IRONWORK".into(), message: m, pos: Pos::default() })?;
        Ok((ending, task))
    }
}

/// Resolves every name before the program runs, so a misspelling is a compile error.
struct Check<'a> {
    layout: &'a Layout,
    program: &'a Program,
    errors: &'a mut Vec<Error>,
}

impl Check<'_> {
    fn statements(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.statement(s);
        }
    }

    fn statement(&mut self, s: &Stmt) {
        match s {
            Stmt::Move { from, to, .. } => {
                self.operand(from);
                to.iter().for_each(|r| self.reference(r));
            }
            Stmt::Compute { targets, expr, size_error, .. } => {
                targets.iter().for_each(|t| self.reference(&t.r));
                self.expr(expr);
                self.size_error(size_error.as_ref());
            }
            Stmt::Arith(a) => {
                for (t, e) in &a.computations {
                    self.reference(&t.r);
                    self.expr(e);
                }
                if let Some((t, x, y)) = &a.remainder {
                    self.reference(&t.r);
                    self.expr(x);
                    self.expr(y);
                }
                self.size_error(a.size_error.as_ref());
            }
            Stmt::If { cond, then, otherwise, .. } => {
                self.cond(cond);
                self.statements(then);
                self.statements(otherwise);
            }
            Stmt::PerformInline { body, repeat, .. } => {
                self.repeat(repeat);
                self.statements(body);
            }
            Stmt::PerformProc { from, thru, repeat, pos } => {
                self.procedure(from, *pos);
                if let Some(t) = thru {
                    self.procedure(t, *pos);
                }
                self.repeat(repeat);
            }
            Stmt::Evaluate { subjects, whens, other, pos } => {
                for subject in subjects {
                    match subject {
                        Subject::Expr(e) => self.expr(e),
                        Subject::Cond(c) => self.cond(c),
                        Subject::Bool(_) => {}
                    }
                }
                for w in whens {
                    for alternative in &w.alternatives {
                        for (subject, object) in subjects.iter().zip(alternative) {
                            match object {
                                Object::Any => {}
                                Object::Bool(_) | Object::Cond(_) if matches!(subject, Subject::Expr(_)) => {
                                    self.errors.push(Error::at(*pos, "a condition as the WHEN object of a value subject"));
                                }
                                Object::Bool(_) => {}
                                Object::Cond(c) => self.cond(c),
                                Object::Value { .. } if !matches!(subject, Subject::Expr(_)) => {
                                    self.errors.push(Error::at(*pos, "a value as the WHEN object of a TRUE, FALSE or condition subject"));
                                }
                                Object::Value { from, thru, .. } => {
                                    self.expr(from);
                                    if let Some(t) = thru {
                                        self.expr(t);
                                    }
                                }
                            }
                        }
                    }
                    self.statements(&w.body);
                }
                self.statements(other);
            }
            Stmt::Display { items, .. } => items.iter().for_each(|o| self.operand(o)),
            Stmt::Open { files, pos } => files.iter().for_each(|(_, f)| self.file(f, *pos)),
            Stmt::Close { files, pos } => files.iter().for_each(|f| self.file(f, *pos)),
            Stmt::Read(r) => {
                self.file(&r.file, r.pos);
                if r.next {
                    self.not_random(&r.file, "READ NEXT", r.pos);
                }
                if let Some(into) = &r.into {
                    self.reference(into);
                }
                if let Some(key) = &r.key {
                    self.reference(key);
                    self.key_of(&r.file, key, false);
                }
                self.handlers(&r.at_end);
                self.handlers(&r.invalid);
            }
            Stmt::Write { record, from, invalid, pos, .. } | Stmt::Rewrite { record, from, invalid, pos } => {
                let verb = if matches!(s, Stmt::Write { .. }) { "WRITE" } else { "REWRITE" };
                self.reference(record);
                if let Ok(layout::Resolved::Item(i)) = self.layout.resolve(&record.name, &record.qualifiers, record.pos)
                    && self.layout.items[i].file.is_none()
                {
                    self.errors.push(Error::at(*pos, format!("{verb} {}: not a record of a file", record.name)));
                }
                if let Some(op) = from {
                    self.operand(op);
                }
                self.handlers(invalid);
            }
            Stmt::Delete { file, invalid, pos } => {
                self.keyed_file(file, "DELETE", *pos);
                self.handlers(invalid);
            }
            Stmt::Start { file, key, invalid, pos } => {
                self.keyed_file(file, "START", *pos);
                self.not_random(file, "START", *pos);
                if let Some((op, r)) = key {
                    if !matches!(op, RelOp::Eq | RelOp::Gt | RelOp::Ge) {
                        self.errors.push(Error::at(*pos, "START KEY takes =, >, NOT < or >="));
                    }
                    self.reference(r);
                    self.key_of(file, r, true);
                }
                self.handlers(invalid);
            }
            Stmt::Initialize { targets, .. } => targets.iter().for_each(|r| self.reference(r)),
            Stmt::GoTo { target, pos } => self.procedure(target, *pos),
            Stmt::Call(c) => {
                self.operand(&c.target);
                for arg in &c.using {
                    if let Some(op) = &arg.value {
                        self.operand(op);
                    }
                }
                if let Some(r) = &c.returning {
                    self.reference(r);
                }
                self.statements(c.on_exception.as_deref().unwrap_or_default());
                self.statements(c.not_on_exception.as_deref().unwrap_or_default());
            }
            Stmt::Cancel { targets, .. } => targets.iter().for_each(|t| self.operand(t)),
            Stmt::Set { set, .. } => match set {
                SetStmt::ConditionTrue(targets) => targets.iter().for_each(|r| self.reference(r)),
                SetStmt::To { targets, value } | SetStmt::AddressOf { targets, value } => {
                    targets.iter().for_each(|r| self.reference(r));
                    self.operand(value);
                }
                SetStmt::UpDown { targets, by, .. } => {
                    targets.iter().for_each(|r| self.reference(r));
                    self.expr(by);
                }
            },
            Stmt::Accept { target, .. } => self.reference(target),
            Stmt::String(st) => {
                for (op, delimiter) in &st.sources {
                    self.operand(op);
                    if let Delimiter::By(d) = delimiter {
                        self.operand(d);
                    }
                }
                self.reference(&st.into);
                if let Some(p) = &st.pointer {
                    self.reference(p);
                }
                self.statements(st.on_overflow.as_deref().unwrap_or_default());
                self.statements(st.not_on_overflow.as_deref().unwrap_or_default());
            }
            Stmt::Unstring(u) => {
                self.reference(&u.source);
                u.delimiters.iter().for_each(|(_, d)| self.operand(d));
                for into in &u.into {
                    self.reference(&into.target);
                    into.delimiter_in.iter().chain(&into.count_in).for_each(|r| self.reference(r));
                }
                u.pointer.iter().chain(&u.tallying).for_each(|r| self.reference(r));
                self.statements(u.on_overflow.as_deref().unwrap_or_default());
                self.statements(u.not_on_overflow.as_deref().unwrap_or_default());
            }
            Stmt::Inspect(i) => {
                self.reference(&i.target);
                for p in i.tallying.iter().chain(&i.replacing) {
                    p.pattern.iter().chain(&p.by).for_each(|o| self.operand(o));
                    if let Some(c) = &p.counter {
                        self.reference(c);
                    }
                    p.bounds.iter().for_each(|b| self.operand(&b.value));
                }
                if let Some((from, to, bounds)) = &i.converting {
                    self.operand(from);
                    self.operand(to);
                    bounds.iter().for_each(|b| self.operand(&b.value));
                }
            }
            Stmt::Search(se) => {
                self.reference_unsubscripted(&se.table);
                if let Some(v) = &se.varying {
                    self.reference(v);
                }
                self.statements(se.at_end.as_deref().unwrap_or_default());
                for (cond, body) in &se.whens {
                    self.cond(cond);
                    self.statements(body);
                }
            }
            Stmt::Exec(block) => self.exec_block(block),
            Stmt::Goback { .. } | Stmt::StopRun { .. } | Stmt::ExitProgram { .. } | Stmt::Continue | Stmt::Exit(_) | Stmt::NextSentence | Stmt::SentenceEnd => {}
        }
    }

    fn size_error(&mut self, se: Option<&SizeError>) {
        if let Some(se) = se {
            self.statements(&se.on);
            self.statements(&se.not_on);
        }
    }

    fn repeat(&mut self, repeat: &Loop) {
        match repeat {
            Loop::Once => {}
            Loop::Times(e) => self.expr(e),
            Loop::Until { cond, .. } => self.cond(cond),
            Loop::Varying { varying, .. } => {
                self.reference(&varying.var);
                self.expr(&varying.from);
                self.expr(&varying.by);
                self.cond(&varying.until);
            }
        }
    }

    fn file(&mut self, name: &str, pos: Pos) {
        if !self.program.files.iter().any(|f| f.name == name) {
            self.errors.push(Error::at(pos, format!("no file named {name}")));
        }
    }

    fn keyed_file(&mut self, name: &str, verb: &str, pos: Pos) {
        match self.program.files.iter().find(|f| f.name == name) {
            None => self.errors.push(Error::at(pos, format!("no file named {name}"))),
            Some(f) if !matches!(f.organization, Organization::Indexed | Organization::Relative) => {
                self.errors.push(Error::at(pos, format!("{verb} {name}: not an indexed or relative file")));
            }
            Some(_) => {}
        }
    }

    fn not_random(&mut self, name: &str, verb: &str, pos: Pos) {
        if self.program.files.iter().any(|f| f.name == name && f.access == Access::Random) {
            self.errors.push(Error::at(pos, format!("{verb} {name}: the file's ACCESS MODE is RANDOM")));
        }
    }

    fn handlers(&mut self, h: &Handlers) {
        self.statements(h.on.as_deref().unwrap_or_default());
        self.statements(h.not_on.as_deref().unwrap_or_default());
    }

    fn item(&self, r: &Ref) -> Option<usize> {
        match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(layout::Resolved::Item(i)) => Some(i),
            _ => None,
        }
    }

    /// The KEY of READ or START on an indexed file: a record key or alternate key of the file, or
    /// for START (`partial`) an item that starts where one does and is no longer.
    fn key_of(&mut self, file: &str, key: &Ref, partial: bool) {
        let Some(f) = self.program.files.iter().find(|f| f.name == file) else { return };
        if f.organization != Organization::Indexed {
            return;
        }
        let Some(item) = self.item(key).map(|i| &self.layout.items[i]) else { return };
        let fits = |r: &Ref| {
            self.item(r).map(|i| &self.layout.items[i]).is_some_and(|k| k.offset == item.offset && (k.size == item.size || partial && item.size < k.size))
        };
        let named = f.record_key.iter().chain(f.alternate_keys.iter().map(|(r, _)| r)).any(fits);
        if item.file.is_none() || !named {
            self.errors.push(Error::at(key.pos, format!("{}: not a key of {file}", key.name)));
        }
    }

    /// An indexed file's keys lie in its records; a relative file's RELATIVE KEY lies outside them.
    fn file_keys(&mut self, k: usize) {
        let f = &self.program.files[k];
        let in_records = |c: &Self, r: &Ref| c.item(r).is_some_and(|i| c.layout.items[i].file == Some(k as u16));
        match f.organization {
            Organization::Indexed => {
                if f.record_key.is_none() {
                    self.errors.push(Error::at(f.pos, format!("{}: an indexed file needs a RECORD KEY", f.name)));
                }
                for r in f.record_key.iter().chain(f.alternate_keys.iter().map(|(r, _)| r)) {
                    self.reference(r);
                    if self.item(r).is_some() && !in_records(self, r) {
                        self.errors.push(Error::at(r.pos, format!("{}: a key of {} must be in its records", r.name, f.name)));
                    }
                }
            }
            Organization::Relative => {
                if let Some(r) = &f.relative_key {
                    self.reference(r);
                    if in_records(self, r) {
                        self.errors.push(Error::at(r.pos, format!("{}: the RELATIVE KEY of {} must not be in its records", r.name, f.name)));
                    }
                } else if f.access != Access::Sequential {
                    self.errors.push(Error::at(f.pos, format!("{}: random or dynamic access needs a RELATIVE KEY", f.name)));
                }
            }
            _ => {}
        }
    }

    fn procedure(&mut self, p: &ProcName, pos: Pos) {
        if let Err(m) = procedure(self.program, p) {
            self.errors.push(Error::at(pos, m));
        }
    }

    fn reference(&mut self, r: &Ref) {
        if r.name == "RETURN-CODE" && r.qualifiers.is_empty() && self.layout.resolve(&r.name, &r.qualifiers, r.pos).is_err() {
            return;
        }
        match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Err(e) => self.errors.push(e),
            Ok(layout::Resolved::Item(i)) if self.layout.items[i].dims.len() != r.subscripts.len() => self.errors.push(Error::at(
                r.pos,
                format!("{} takes {} subscripts, not {}", r.name, self.layout.items[i].dims.len(), r.subscripts.len()),
            )),
            Ok(_) => {}
        }
        r.subscripts.iter().for_each(|e| self.expr(e));
        if let Some(rm) = &r.refmod {
            self.expr(&rm.start);
            if let Some(l) = &rm.length {
                self.expr(l);
            }
        }
    }

    /// Every host variable and every CICS argument that names data must resolve.
    fn exec_block(&mut self, block: &ExecBlock) {
        if let Some(syntax::sql::Sql { statement: syntax::sql::Statement::Malformed(why), .. }) = &block.sql {
            self.errors.push(Error::at(block.pos, format!("EXEC SQL {}: {why}", block.command)));
        }
        for r in &block.host_variables {
            if r.subscripts.is_empty() {
                self.reference_unsubscripted(r);
            } else {
                self.reference(r);
            }
        }
        for (_, arg) in &block.options {
            if let Some(ExecArg::Operand(op)) = arg {
                self.operand(op);
            }
        }
    }

    /// SEARCH names a table without a subscript.
    fn reference_unsubscripted(&mut self, r: &Ref) {
        if let Err(e) = self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            self.errors.push(e);
        }
    }

    fn operand(&mut self, op: &Operand) {
        match op {
            Operand::Ref(r) | Operand::LengthOf(r) | Operand::AddressOf(r) => self.reference(r),
            Operand::Literal(Literal::Number(t)) if machine::literal_fixed(t).is_none() => {
                self.errors.push(Error::at(Pos::default(), format!("the literal {t} has more than 31 digits")));
            }
            Operand::Literal(_) => {}
            Operand::Function(f) => {
                if !FUNCTIONS.contains(&f.name.as_str()) {
                    self.errors.push(Error::at(f.pos, format!("FUNCTION {} is not supported yet", f.name)));
                }
                f.args.iter().for_each(|a| self.expr(a));
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Operand(op) => self.operand(op),
            Expr::Neg(inner) => self.expr(inner),
            Expr::Bin(a, _, b) => {
                self.expr(a);
                self.expr(b);
            }
        }
    }

    fn cond(&mut self, c: &Cond) {
        match c {
            Cond::Rel(a, _, b) => {
                self.expr(a);
                self.expr(b);
            }
            Cond::Class(e, _) => self.expr(e),
            Cond::Name(r) => self.reference(r),
            Cond::NameOrRel { subject, name, .. } => {
                self.expr(subject);
                self.reference(name);
            }
            Cond::Not(inner) => self.cond(inner),
            Cond::And(a, b) | Cond::Or(a, b) => {
                self.cond(a);
                self.cond(b);
            }
        }
    }
}

#[cfg(test)]
mod tests;
