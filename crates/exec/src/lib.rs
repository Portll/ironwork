//! ironwork for COBOL: WORKING-STORAGE laid out as IBM lays it out, and an interpreter that runs a
//! program against it in EBCDIC with the numeric model of `ironwork-numeric`.

pub mod abend;
pub use rt::calendar;
pub mod cics;
pub use rt::codec;
pub use rt::digest;
pub use rt::evidence;
pub mod collating;
pub mod declaratives;
pub mod edit;
pub mod files;
pub mod layout;
pub mod le;
pub mod linage;
pub mod machine;
pub mod oo;
pub mod picture;
pub mod printer;
pub mod report;
mod reserved;
mod sort;
pub mod sql;
pub use rt::strings;
pub mod terminal;
#[cfg(test)]
mod testing;
pub mod tn3270;
pub mod unit;

pub use machine::{Abend, Ending};

use abend::AbendCode;
use layout::Layout;
use numeric::Options;
use std::io::{BufRead, Write};
use syntax::ast::*;
use syntax::{Error, Pos, Severity};

pub struct Compiled {
    pub program: Program,
    pub layout: Layout,
    pub options: Options,
    pub ssrange: bool,
    pub report_writer: report::Writer,
    /// The PROGRAM COLLATING SEQUENCE, or EBCDIC.
    pub collating: collating::Sequence,
    /// Each file's printer control character, when it is a print file.
    pub carriage: Vec<Option<printer::Carriage>>,
    /// The warnings and informational messages of a program that compiled.
    pub diagnostics: Vec<Error>,
    /// The program's ENTRY statements, in source order.
    pub entries: Vec<EntryPoint>,
    /// Where each EXCEPTION/ERROR and debugging procedure runs.
    pub declaratives: declaratives::Table,
}

/// An alternate entry point: a CALL of `name` begins at statement `statement` of paragraph
/// `paragraph`, the one after the ENTRY statement, with `using` addressing LINKAGE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryPoint {
    pub name: String,
    pub paragraph: usize,
    pub statement: usize,
    pub using: Vec<Param>,
    pub pos: Pos,
}

/// The ENTRY statements of a program, each a sentence of its own paragraph's.
pub fn entry_points(program: &Program) -> Vec<EntryPoint> {
    let mut out = Vec::new();
    for (paragraph, p) in program.paragraphs.iter().enumerate() {
        for (k, s) in p.statements.iter().enumerate() {
            if let Stmt::Entry { name, using, pos } = s {
                out.push(EntryPoint { name: name.clone(), paragraph, statement: k + 1, using: using.clone(), pos: *pos });
            }
        }
    }
    out
}

const FUNCTIONS: &[&str] = &[
    "CHAR", "ORD", "NATIONAL-OF", "LENGTH", "UPPER-CASE", "LOWER-CASE", "REVERSE", "CURRENT-DATE", "NUMVAL", "NUMVAL-C", "TRIM", "MOD", "REM",
    "INTEGER", "INTEGER-PART", "ABS", "MIN", "MAX", "INTEGER-OF-DATE", "DATE-OF-INTEGER", "RANDOM",
];

/// Checks and lays out a parsed program. `flags` are this compiler's own, such as `-silent`. A
/// program is refused, with every message, when one is an error (E, S or U), or under
/// `-warnings-block` a warning; otherwise its warnings and informational messages are
/// [`Compiled::diagnostics`].
pub fn compile(program: Program, flags: &[String]) -> Result<Compiled, Vec<Error>> {
    if program.oo.as_ref().is_some_and(|o| o.class().is_some()) {
        return oo::compile_class_definition(program, flags);
    }
    compile_program(program, flags, true)
}

/// `whole` is false for the parts a class definition is compiled into, which IBM's rules for
/// compiler options do not apply to one by one.
pub(crate) fn compile_program(program: Program, flags: &[String], whole: bool) -> Result<Compiled, Vec<Error>> {
    let mut errors = Vec::new();
    reserved::check(&program, &mut errors);
    let mut program = declaratives::with_debug_item(sort::with_special_registers(program));
    let mut options = Options::default();
    let mut ssrange = false;
    for option in &program.options {
        if let Some(on) = numeric::options::switch(option, "SSRANGE") {
            ssrange = on;
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
    for (name, alphabet) in &program.environment.alphabets {
        if program.environment.collating_sequence.as_ref() != Some(name)
            && let Err(m) = collating::Sequence::of(alphabet, options.code_page())
        {
            errors.push(Error::at(Pos::default(), format!("ALPHABET {name}: {m}")));
        }
    }
    let collating = collating::Sequence::program(&program.environment, options.code_page()).unwrap_or_else(|m| {
        errors.push(Error::at(Pos::default(), m));
        collating::Sequence::native()
    });
    digit_limits(&program, options.arith, &mut errors);
    let drafts = report::prepare(&mut program, options.adv, &mut errors);
    let linage_counters = linage::add_counters(&mut program);
    if whole {
        oo::option_rules(&program, &options, &mut errors);
    }
    let files: Vec<(&[DataEntry], Option<u32>)> = program.files.iter().map(|f| (f.records.as_slice(), f.record_max)).collect();
    let shared = layout::record_area_owners(&program.files, &program.environment).unwrap_or_else(|e| {
        errors.push(e);
        (0..files.len()).collect()
    });
    let mut layout = match layout::build(&program.working_storage, &files, &shared, &program.linkage, &program.local_storage, program.environment.decimal_point_comma) {
        Ok(l) => l,
        Err(e) => {
            errors.push(e);
            return Err(errors.into_iter().map(|e| e.in_files(&program.sources)).collect());
        }
    };
    let counter_item = |entry: usize| program.working_storage[..entry].iter().filter(|e| e.level != 88).count();
    layout.name_files(&program.files, linage_counters.iter().map(|c| c.map(counter_item)).collect());
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
    let report_writer = report::resolve(&program, &layout, drafts, &mut errors);
    let carriage = printer::carriages(&program, &layout, options.adv);
    let declaratives = declaratives::resolve(&program, &layout, &options, &mut errors);
    let debugging = declaratives::debugging_sections(&program);
    let mut check = Check { layout: &layout, program: &program, errors: &mut errors, debugging: false, max_digits: options.arith.max_picture_digits() };
    for k in 0..program.files.len() {
        check.file_keys(k);
        linage::check_file(check.program, check.layout, k, check.errors);
    }
    for block in &program.exec_declarations {
        check.exec_block(block);
    }
    for (i, p) in program.paragraphs.iter().enumerate() {
        check.debugging = debugging.iter().any(|&(first, last)| (first..=last).contains(&i));
        check.statements(&p.statements);
    }
    let entries = entry_points(&program);
    procedure_rules(&program, &layout, &entries, &options, &mut errors);
    oo::check(&layout, &program, &mut errors);
    let errors: Vec<Error> = errors.into_iter().map(|e| e.in_files(&program.sources)).collect();
    if refused(&errors, &options) {
        Err(errors)
    } else {
        Ok(Compiled { program, layout, options, ssrange, report_writer, collating, carriage, diagnostics: errors, entries, declaratives })
    }
}

/// Whether messages keep a program from running: an error does, and a warning under
/// `-warnings-block`. See [`numeric::assumptions::REFUSED_FROM_E`].
pub(crate) fn refused(messages: &[Error], options: &Options) -> bool {
    let floor = match options.warnings {
        numeric::options::Warnings::Proceed => Severity::Error,
        numeric::options::Warnings::Block => Severity::Warning,
    };
    messages.iter().any(|m| m.severity >= floor)
}

/// The rules of the ENTRY statement (Language Reference SC27-8713-03, pp. 339-340), and of ALTER
/// and the GO TO it alters (pp. 318, 346-348).
fn procedure_rules(program: &Program, layout: &Layout, entries: &[EntryPoint], options: &Options, errors: &mut Vec<Error>) {
    let method = program.oo.as_ref().is_some_and(|o| matches!(o.unit, OoUnit::Method(_)));
    for (k, e) in entries.iter().enumerate() {
        if program.returning.is_some() {
            errors.push(Error::at(e.pos, format!("ENTRY '{}': a program with PROCEDURE DIVISION RETURNING cannot have ENTRY statements", e.name)));
        }
        if e.name.eq_ignore_ascii_case(&program.id) || entries[..k].iter().any(|f| f.name == e.name) {
            errors.push(Error::at(e.pos, format!("ENTRY '{}': the name is already the program's or another ENTRY's", e.name)));
        }
        for param in &e.using {
            if !layout.linkage_roots.iter().any(|&i| layout.items[i].name.as_deref() == Some(param.name.as_str())) {
                errors.push(Error::at(e.pos, format!("ENTRY '{}' USING {}: not an 01 or 77 item of the LINKAGE SECTION", e.name, param.name)));
            }
        }
    }
    let why_no_alter = if program.recursive {
        Some("a RECURSIVE program")
    } else if options.thread {
        Some("a program compiled with THREAD")
    } else {
        method.then_some("a method")
    };
    for p in &program.paragraphs {
        for s in &p.statements {
            let mut inner = Vec::new();
            oo::bodies(s).into_iter().for_each(|body| inner_statements(body, &mut inner));
            for (t, nested) in std::iter::once((s, false)).chain(inner.into_iter().map(|t| (t, true))) {
                match t {
                    Stmt::Entry { name, pos, .. } if nested => {
                        errors.push(Error::at(*pos, format!("ENTRY '{name}' must be a sentence of its own, not inside another statement")));
                    }
                    Stmt::GoTo { target: None, pos } => {
                        if let Some(why) = why_no_alter {
                            errors.push(Error::at(*pos, format!("a GO TO with no procedure-name cannot be used in {why}")));
                        }
                        if nested || !lone_go_to(p) {
                            errors.push(Error::at(*pos, "a GO TO with no procedure-name must be its paragraph's only sentence"));
                        }
                    }
                    Stmt::Alter { pairs, pos } => {
                        if let Some(why) = why_no_alter {
                            errors.push(Error::at(*pos, format!("ALTER cannot be used in {why}")));
                        }
                        for (from, to) in pairs {
                            altered_paragraph(program, from, *pos, errors);
                            if let Err(m) = procedure(program, to) {
                                errors.push(Error::at(*pos, m));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Every statement inside `stmts`, at any depth.
fn inner_statements<'s>(stmts: &'s [Stmt], out: &mut Vec<&'s Stmt>) {
    for s in stmts {
        out.push(s);
        oo::bodies(s).into_iter().for_each(|body| inner_statements(body, out));
    }
}

/// A paragraph ALTER can name: one sentence, a GO TO without DEPENDING ON.
fn altered_paragraph(program: &Program, name: &ProcName, pos: Pos, errors: &mut Vec<Error>) {
    match procedure(program, name) {
        Err(m) => errors.push(Error::at(pos, m)),
        Ok((i, _)) if program.paragraphs[i].is_section => errors.push(Error::at(pos, format!("ALTER {}: a section, where ALTER names a paragraph", name.name))),
        Ok((i, _)) if !lone_go_to(&program.paragraphs[i]) => {
            errors.push(Error::at(pos, format!("ALTER {}: the paragraph must hold one sentence, a GO TO without DEPENDING ON", name.name)));
        }
        Ok(_) => {}
    }
}

fn lone_go_to(p: &Paragraph) -> bool {
    matches!(p.statements.as_slice(), [Stmt::GoTo { .. }] | [Stmt::GoTo { .. }, Stmt::SentenceEnd])
}

/// The last paragraph of the section that paragraph `i` is in, or `i` when there are no sections.
/// END DECLARATIVES ends a section as a section header does.
pub(crate) fn section_end(program: &Program, i: usize) -> usize {
    let paragraphs = &program.paragraphs;
    let declaratives = program.report_writer.procedure_start;
    let (floor, ceiling) = if i < declaratives { (0, declaratives) } else { (declaratives, paragraphs.len()) };
    let Some(header) = (floor..=i).rev().find(|&j| paragraphs[j].is_section) else { return i };
    (header + 1..ceiling).take_while(|&j| !paragraphs[j].is_section).last().unwrap_or(header)
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
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend> {
        self.execute_observed(library, dds, sysin, clock, database, out, err, None)
    }

    /// Runs as [`Compiled::execute_with`] does, telling `observer` what the run opens, closes and
    /// loads.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_observed<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
    ) -> Result<(Ending, i16), Abend> {
        oo::refuse_to_run(&self.program)?;
        let mut run_unit = unit::RunUnit::new(library, dds, sysin, clock, out, err);
        run_unit.observer = observer;
        run_unit.sql = database.map(sql::Session::new);
        let me = run_unit.add(None, &self.program, self.layout.size as usize);
        let ending = machine::Machine::activation(self, me, &mut run_unit, true).and_then(|mut m| m.run_procedure());
        let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(&self.program.id, ending.is_ok()).map(drop));
        let closed = run_unit.close_all();
        let ending = ending?;
        settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default() })?;
        closed.map_err(|m| Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default() })?;
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
    /// back after an abend, and closes every cursor. The database outlives the task, so a region's
    /// tasks can share one.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_cics_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        mut task: cics::Task,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend> {
        oo::refuse_to_run(&self.program)?;
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
        let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.end_task(&self.program.id, ending.is_ok()).map(drop));
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
        let ending = ending.map_err(|a| match a.code {
            AbendCode::Check(_) | AbendCode::Protection => Abend { message: format!("{} ({}, which CICS reports as ASRA)", a.message, a.code), code: AbendCode::Cics("ASRA".into()), pos: a.pos },
            _ => a,
        })?;
        settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default() })?;
        closed.map_err(|m| Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default() })?;
        Ok((ending, task))
    }
}

/// Under ARITH(COMPAT) a numeric or numeric-edited PICTURE, scaling positions P included, and a
/// fixed-point numeric literal hold at most 18 digits, and under ARITH(EXTEND) 31 (Language
/// Reference SC27-8713-03, pp. 45, 209, 217-218; Programming Guide SC27-8714-03, p. 349).
fn digit_limits(program: &Program, arith: numeric::options::Arith, errors: &mut Vec<Error>) {
    let max = arith.max_picture_digits();
    let option = match arith {
        numeric::options::Arith::Compat => "ARITH(COMPAT)",
        numeric::options::Arith::Extend => "ARITH(EXTEND)",
    };
    let entries = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records));
    for e in entries {
        if let Some(p) = e.picture.as_deref()
            && let Ok(pic) = picture::analyse_with(p, program.environment.decimal_point_comma)
            && matches!(pic.category, picture::Category::Numeric | picture::Category::NumericEdited)
        {
            let positions = pic.digits + pic.scaling + pic.scale.saturating_sub(pic.digits);
            if positions > max {
                errors.push(Error::at(e.pos, format!("PICTURE {p}: {positions} digit positions, more than the {max} {option} allows")));
            }
        }
        let values = e.value.iter().chain(e.condition_values.iter().flat_map(|(low, high)| std::iter::once(low).chain(high))).chain(&e.false_value);
        for v in values {
            if let Literal::Number(t) = v
                && literal_digits(t) > max as usize
            {
                errors.push(Error::at(e.pos, format!("the literal {t} has more than the {max} digits {option} allows")));
            }
        }
    }
}

fn literal_digits(t: &str) -> usize {
    t.chars().filter(char::is_ascii_digit).count()
}

/// Resolves every name before the program runs, so a misspelling is a compile error.
struct Check<'a> {
    layout: &'a Layout,
    program: &'a Program,
    errors: &'a mut Vec<Error>,
    /// The statements are a debugging section's, which alone may reference DEBUG-ITEM.
    debugging: bool,
    /// The most digits a numeric literal has under the program's ARITH option.
    max_digits: u32,
}

impl Check<'_> {
    fn statements(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.statement(s);
        }
    }

    fn statement(&mut self, s: &Stmt) {
        linage::check_receivers(self.layout, s, self.errors);
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
                if let Stmt::Write { advancing, end_of_page, .. } = s {
                    if let Some(a) = advancing {
                        if let Advancing::Lines { count, .. } = a {
                            self.expr(count);
                        }
                        printer::check_write(self.program, self.layout, record, a, *pos, self.errors);
                    }
                    linage::check_write(self.program, self.layout, record, advancing.as_ref(), end_of_page, *pos, self.errors);
                    self.handlers(end_of_page);
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
            Stmt::Initialize { targets, pos } => {
                for r in targets {
                    self.reference(r);
                    if self.item(r).is_some_and(|i| self.layout.items[i].level == 66) {
                        self.errors.push(Error::at(*pos, format!("INITIALIZE {}: a level-66 RENAMES item cannot be initialized", r.name)));
                    }
                }
            }
            Stmt::GoTo { target: Some(target), pos } => self.procedure(target, *pos),
            Stmt::GoToDepending { targets, on, pos } => {
                targets.iter().for_each(|t| self.procedure(t, *pos));
                self.reference(on);
            }
            Stmt::GoTo { target: None, .. } | Stmt::Alter { .. } | Stmt::Entry { .. } => {}
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
                SetStmt::ConditionFalse(targets) => {
                    for r in targets {
                        self.reference(r);
                        if let Ok(layout::Resolved::Condition(c)) = self.layout.resolve(&r.name, &r.qualifiers, r.pos)
                            && self.layout.conditions[c].false_value.is_none()
                        {
                            self.errors.push(Error::at(r.pos, format!("SET {} TO FALSE: the condition-name has no WHEN SET TO FALSE value", r.name)));
                        }
                    }
                }
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
            Stmt::Report(r) => report::check_statement(self.program, r, self.errors),
            Stmt::Invoke(i) => self.invoke(i),
            Stmt::Sorting(s) => self.sorting(s),
            Stmt::Goback { .. } | Stmt::StopRun { .. } | Stmt::ExitProgram { .. } | Stmt::ExitMethod { .. } | Stmt::Continue | Stmt::Exit(_) | Stmt::NextSentence | Stmt::SentenceEnd => {}
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
            Loop::Varying { varying, after, .. } => {
                for v in std::iter::once(&**varying).chain(after) {
                    self.reference(&v.var);
                    self.expr(&v.from);
                    self.expr(&v.by);
                    self.cond(&v.until);
                }
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
        if oo::special_register(self.layout, r) {
            return;
        }
        if !self.debugging && !self.program.declaratives.debugging.is_empty() && declaratives::DEBUG_ITEM_NAMES.contains(&r.name.as_str()) {
            self.errors.push(Error::at(r.pos, format!("{}: only a debugging section may reference DEBUG-ITEM", r.name)));
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
        if block.kind == ExecKind::Dli {
            self.errors.push(Error::at(block.pos, format!("EXEC DLI {} is not supported: ironwork for COBOL does not run IMS DL/I calls", block.command)));
        }
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
            Operand::Literal(Literal::Number(t)) if machine::literal_fixed(t).is_none() || literal_digits(t) > self.max_digits as usize => {
                self.errors.push(Error::at(Pos::default(), format!("the literal {t} has more than {} digits", self.max_digits.min(31))));
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
