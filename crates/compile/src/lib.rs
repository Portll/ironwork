//! ironwork for COBOL, the compiler: a parsed program checked against IBM's rules, with its
//! WORKING-STORAGE laid out as IBM lays it out, ready for the interpreter or for lowering.

pub mod collating;
mod corresponding;
pub mod declaratives;
pub mod layout;
pub mod linage;
pub mod markup;
pub mod oo;
pub mod picture;
pub mod printer;
pub mod report;
mod reserved;
pub mod sort;
pub mod sql;

use layout::Layout;
use numeric::Options;
use rt::lir::{CompileTime, TimeSource};
use rt::storage::literal_fixed;
use syntax::ast::*;
use syntax::{Error, Pos, Severity};

pub struct Compiled {
    pub program: Program,
    /// FUNCTION WHEN-COMPILED's time.
    pub when_compiled: CompileTime,
    pub layout: Layout,
    pub options: Options,
    pub ssrange: bool,
    pub report_writer: report::Writer,
    /// The PROGRAM COLLATING SEQUENCE, or EBCDIC.
    pub collating: collating::Sequence,
    /// Each file's printer control character, when it is a print file.
    pub carriage: Vec<Option<printer::Carriage>>,
    /// The messages of a program that compiled, none of them severe enough to stop its object code.
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
/// program is refused, with every message, when one stops its object code: under IBM's default
/// NOCOMPILE(S) one that is S or U, from W under `-warnings-block`, or as a CBL or PROCESS card's
/// COMPILE or NOCOMPILE says; otherwise its messages are [`Compiled::diagnostics`].
pub fn compile(program: Program, flags: &[String]) -> Result<Compiled, Vec<Error>> {
    let at = compile_time().map_err(|message| vec![Error::at(Pos::default(), message)])?;
    compile_at(program, flags, at)
}

/// Compiles as [`compile`] does, WHEN-COMPILED giving `at`.
pub fn compile_at(program: Program, flags: &[String], at: CompileTime) -> Result<Compiled, Vec<Error>> {
    if program.oo.as_ref().is_some_and(|o| o.class().is_some()) {
        return oo::compile_class_definition(program, flags, at);
    }
    compile_program(program, flags, true, at)
}

/// When a compile happens: SOURCE_DATE_EPOCH's seconds when the build sets it, the
/// reproducible-builds convention, and the clock otherwise.
pub fn compile_time() -> Result<CompileTime, String> {
    let clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    compile_time_from(std::env::var_os("SOURCE_DATE_EPOCH").as_deref(), clock)
}

fn compile_time_from(epoch: Option<&std::ffi::OsStr>, clock: std::time::Duration) -> Result<CompileTime, String> {
    let Some(epoch) = epoch else {
        return Ok(CompileTime { seconds: clock.as_secs() as i64, hundredths: clock.subsec_millis() / 10, source: TimeSource::Clock });
    };
    let text = epoch.to_string_lossy();
    match text.parse::<i64>() {
        Ok(seconds) if text.bytes().all(|b| b.is_ascii_digit()) && seconds <= CompileTime::LATEST => Ok(CompileTime { seconds, hundredths: 0, source: TimeSource::SourceDateEpoch }),
        _ => Err(format!("SOURCE_DATE_EPOCH={text}: not a whole number of seconds from 0 to {}", CompileTime::LATEST)),
    }
}

/// `whole` is false for the parts a class definition is compiled into, which IBM's rules for
/// compiler options do not apply to one by one.
pub(crate) fn compile_program(mut program: Program, flags: &[String], whole: bool, when_compiled: CompileTime) -> Result<Compiled, Vec<Error>> {
    let mut errors = std::mem::take(&mut program.messages);
    reserved::check(&program, &mut errors);
    let mut program = declaratives::with_debug_item(markup::with_special_registers(sort::with_special_registers(program)));
    qualify_in_own_section(&mut program);
    let mut options = Options::default();
    let mut ssrange = false;
    for option in &program.options {
        if let Some(on) = numeric::options::switch(option, "SSRANGE") {
            ssrange = on;
        }
        if let Err(e) = options.apply(option) {
            errors.push(Error::at(Pos::default(), format!("CBL {option}: {e}")).graded(option_severity(&e)));
        }
    }
    for flag in flags {
        if let Err(e) = options.apply_flag(flag) {
            errors.push(Error::at(Pos::default(), e.to_string()));
        }
    }
    default_currency(&mut program, &mut options, &mut errors);
    national_symbols(&program, &mut options, &mut errors);
    for (name, alphabet) in &program.environment.alphabets {
        if program.environment.collating_sequence.as_ref() != Some(name)
            && let Err(m) = collating::Sequence::of(alphabet, options.code_page(), options.quote)
        {
            errors.push(Error::at(Pos::default(), format!("ALPHABET {name}: {m}")));
        }
    }
    let collating = collating::Sequence::program(&program.environment, options.code_page(), options.quote).unwrap_or_else(|m| {
        errors.push(Error::at(Pos::default(), m));
        let mut native = collating::Sequence::native();
        native.quote = options.quote;
        native
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
    let mut layout = match layout::build(&program.working_storage, &files, &shared, &program.linkage, &program.local_storage, crate::picture::Notation::of(&program.environment)) {
        Ok(l) => l,
        Err(e) => {
            errors.push(e);
            return Err(errors.into_iter().map(|e| e.in_files(&program.sources)).collect());
        }
    };
    let counter_item = |entry: usize| program.working_storage[..entry].iter().filter(|e| e.level != 88).count();
    layout.name_files(&program.files, linage_counters.iter().map(|c| c.map(counter_item)).collect());
    corresponding::expand(&mut program, &layout, &mut errors);
    condition_subjects(&mut program, &layout);
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
    let mut check = Check { layout: &layout, program: &program, errors: &mut errors, debugging: false, max_digits: options.arith.max_picture_digits(), inline_performs: 0 };
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
    if whole {
        program_end(&program, &options, &mut errors);
    }
    oo::check(&layout, &program, &mut errors);
    let errors: Vec<Error> = errors.into_iter().map(|e| e.in_files(&program.sources)).collect();
    if refused(&errors, &options) {
        Err(errors)
    } else {
        Ok(Compiled { program, when_compiled, layout, options, ssrange, report_writer, collating, carriage, diagnostics: errors, entries, declaratives })
    }
}

/// How severe IBM's message for a card's option is: an invalid suboption is an error and the option
/// is discarded, and a removed option a warning or informational message (assumptions
/// [`numeric::assumptions::INVALID_OPTION_DISCARDED`], [`numeric::assumptions::NUMPROC_MIG_WARNS`]
/// and [`numeric::assumptions::OPTIONS_WITHOUT_EFFECT`]). A code page ironwork does not carry
/// stops the compile, as ironwork cannot read the program in it.
fn option_severity(e: &numeric::options::OptionError) -> Severity {
    use numeric::options::OptionError;
    match e {
        OptionError::BadSuboption { .. } => Severity::Error,
        OptionError::Removed { .. } | OptionError::NoEffect { warning: true, .. } => Severity::Warning,
        OptionError::NoEffect { warning: false, .. } => Severity::Informational,
        OptionError::UnsupportedCodePage(_) | OptionError::UnknownFlag(_) => Severity::Severe,
    }
}

/// CURRENCY(literal) makes its character the currency symbol, standing for itself, of a program
/// with no CURRENCY SIGN clause, in place of $; a program with one ignores the option
/// (Programming Guide SC27-8714-03, p. 358). A hexadecimal literal whose character the option may
/// not name is discarded with an error, as an invalid suboption is (assumption
/// [`numeric::assumptions::CURRENCY_OPTION`]).
fn default_currency(program: &mut Program, options: &mut Options, errors: &mut Vec<Error>) {
    match options.currency_symbol() {
        Some(Ok(symbol)) if program.environment.currency.is_empty() => program.environment.currency.push(CurrencySign { value: symbol.to_string(), symbol }),
        Some(Err(c)) => {
            errors.push(Error::at(Pos::default(), format!("CBL CURRENCY: code page {} reads its byte as {c:?}, which cannot be a currency symbol", options.codepage)).graded(Severity::Error));
            options.currency = None;
        }
        _ => {}
    }
}

/// NSYMBOL(DBCS) makes a PICTURE of N alone with no USAGE, its own or a group's, USAGE DISPLAY-1
/// (Programming Guide SC27-8714-03, p. 388), which ironwork does not have. NSYMBOL(NATIONAL) with
/// NODBCS on the cards is IBM's conflict: an error, and DBCS in effect (p. 344).
fn national_symbols(program: &Program, options: &mut Options, errors: &mut Vec<Error>) {
    if options.nsymbol == numeric::Nsymbol::National {
        if !options.dbcs && program.options.iter().any(|o| numeric::options::switch(o, "NSYMBOL").is_some()) {
            errors.push(Error::at(Pos::default(), "CBL NODBCS: NSYMBOL(NATIONAL) requires DBCS, which is in effect").graded(Severity::Error));
            options.dbcs = true;
        }
        return;
    }
    let only_n = |p: &str| p.chars().any(|c| c.eq_ignore_ascii_case(&'N')) && p.chars().all(|c| c.eq_ignore_ascii_case(&'N') || c.is_ascii_digit() || matches!(c, '(' | ')'));
    let lists = [&program.working_storage, &program.local_storage, &program.linkage].into_iter().chain(program.files.iter().map(|f| &f.records));
    for entries in lists {
        let mut groups: Vec<(u8, bool)> = Vec::new();
        for e in entries.iter().filter(|e| !matches!(e.level, 66 | 88)) {
            let level = if e.level == 77 { 1 } else { e.level };
            while groups.last().is_some_and(|&(l, _)| l >= level) {
                groups.pop();
            }
            let usage = e.usage.is_some() || groups.iter().any(|&(_, u)| u);
            if let Some(p) = e.picture.as_deref().filter(|p| !usage && only_n(p)) {
                errors.push(Error::at(e.pos, format!("PICTURE {p} with no USAGE is DISPLAY-1 under NSYMBOL(DBCS), and ironwork has no DBCS data")));
            }
            groups.push((level, e.usage.is_some()));
        }
    }
}

/// IBM's warning for a program with no STOP RUN, GOBACK or EXIT PROGRAM, which may run past its
/// end; one that leaves by EXEC CICS RETURN or XCTL gets what `--cics-return-warning` says
/// (assumption [`numeric::assumptions::NO_PROGRAM_END`]).
fn program_end(program: &Program, options: &Options, errors: &mut Vec<Error>) {
    use numeric::CicsReturnWarning;
    let mut all = Vec::new();
    program.paragraphs.iter().for_each(|p| inner_statements(&p.statements, &mut all));
    if all.iter().any(|s| matches!(s, Stmt::StopRun { .. } | Stmt::Goback { .. } | Stmt::ExitProgram { .. })) {
        return;
    }
    let cics_end = all.iter().find_map(|s| match s {
        Stmt::Exec(b) if b.kind == ExecKind::Cics && matches!(b.command.as_str(), "RETURN" | "XCTL") => Some(b.command.as_str()),
        _ => None,
    });
    match (cics_end, options.cics_return_warning) {
        (Some(_), CicsReturnWarning::Never) => {}
        (Some(command), CicsReturnWarning::Once) => errors.push(Error::at(Pos::default(), format!(
            "IGYPS2091-W not given: the program ends with EXEC CICS {command}, which the CICS translator turns into a CALL; --cics-return-warning=always gives the warning, =never drops this note"
        )).graded(Severity::Informational)),
        (None, _) | (Some(_), CicsReturnWarning::Always) => {
            errors.push(Error::warning(Pos::default(), "no STOP RUN, GOBACK or EXIT PROGRAM in the program: check that it ends"));
        }
    }
}

/// Whether messages keep a program from running: the COMPILE option in force stops its object code,
/// or, as IGYWCLG bypasses its GO step, the return code is above 8. See
/// [`numeric::assumptions::REFUSED_FROM_S`] and [`numeric::assumptions::NOCOMPILE`].
pub fn refused(messages: &[Error], options: &Options) -> bool {
    let stops_at = options.object_code().stops_at().min(12);
    stops_at == 0 || messages.iter().any(|m| m.severity.return_code() >= stops_at)
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
pub fn section_end(program: &Program, i: usize) -> usize {
    let paragraphs = &program.paragraphs;
    let declaratives = program.report_writer.procedure_start;
    let (floor, ceiling) = if i < declaratives { (0, declaratives) } else { (declaratives, paragraphs.len()) };
    let Some(header) = (floor..=i).rev().find(|&j| paragraphs[j].is_section) else { return i };
    (header + 1..ceiling).take_while(|&j| !paragraphs[j].is_section).last().unwrap_or(header)
}

/// Qualifies each unqualified paragraph-name that more than one section holds with the section it
/// is written in, when that section holds it: within its own section a paragraph-name needs no
/// qualifier, so every later lookup must find that paragraph.
fn qualify_in_own_section(program: &mut Program) {
    let paragraphs: Vec<(String, Option<String>, bool)> = program.paragraphs.iter().map(|p| (p.name.clone(), p.section.clone(), p.is_section)).collect();
    let in_section = |name: &str, section: &str| paragraphs.iter().any(|(n, s, is_section)| n == name && !is_section && s.as_deref() == Some(section));
    let named = |name: &str| paragraphs.iter().filter(|(n, ..)| n == name).count();
    for p in &mut program.paragraphs {
        let Some(section) = p.section.clone() else { continue };
        oo::each_mut(&mut p.statements, &mut |s| {
            for name in procedure_names_mut(s) {
                if name.section.is_none() && named(&name.name) > 1 && in_section(&name.name, &section) {
                    name.section = Some(section.clone());
                }
            }
        });
    }
}

/// An EVALUATE subject that is a condition-name is a condition, and so are its WHEN objects
/// written as names, which the parser could not tell from values.
fn condition_subjects(program: &mut Program, layout: &Layout) {
    let is_condition = |r: &Ref| matches!(layout.resolve(&r.name, &r.qualifiers, r.pos), Ok(layout::Resolved::Condition(_)));
    for p in &mut program.paragraphs {
        oo::each_mut(&mut p.statements, &mut |s| {
            let Stmt::Evaluate { subjects, whens, .. } = s else { return };
            for (k, subject) in subjects.iter_mut().enumerate() {
                let Subject::Expr(Expr::Operand(Operand::Ref(r))) = subject else { continue };
                if !is_condition(r) {
                    continue;
                }
                *subject = Subject::Cond(Cond::Name(r.clone()));
                for alternative in whens.iter_mut().flat_map(|w| w.alternatives.iter_mut()) {
                    let cond = match alternative.get(k) {
                        Some(Object::Value { not, from: Expr::Operand(Operand::Ref(o)), thru: None }) => {
                            let c = Cond::Name(o.clone());
                            if *not { Cond::Not(Box::new(c)) } else { c }
                        }
                        _ => continue,
                    };
                    alternative[k] = Object::Cond(cond);
                }
            }
        });
    }
}

/// The procedure-names statement `s` itself names, not those of the statements it holds.
fn procedure_names_mut(s: &mut Stmt) -> Vec<&mut ProcName> {
    match s {
        Stmt::PerformProc { from, thru, .. } => std::iter::once(from).chain(thru.as_mut()).collect(),
        Stmt::GoTo { target, .. } => target.iter_mut().collect(),
        Stmt::GoToDepending { targets, .. } => targets.iter_mut().collect(),
        Stmt::Alter { pairs, .. } => pairs.iter_mut().flat_map(|(from, to)| [from, to]).collect(),
        Stmt::XmlParse(x) => std::iter::once(&mut x.procedure).chain(x.thru.as_mut()).collect(),
        Stmt::Sorting(so) => match &mut **so {
            Sorting::Sort(st) => [st.input.as_mut(), st.output.as_mut()]
                .into_iter()
                .flatten()
                .flat_map(|io| match io {
                    SortIo::Procedure { from, thru } => std::iter::once(from).chain(thru.as_mut()).collect(),
                    SortIo::Files(_) => Vec::new(),
                })
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The first and last paragraph a procedure name covers: one paragraph, or a whole section.
pub fn procedure(program: &Program, p: &ProcName) -> Result<(usize, usize), String> {
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
            && let Ok(pic) = picture::analyse_with(p, crate::picture::Notation::of(&program.environment))
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
/// The condition-names a JSON PARSE USING phrase names.
fn flag_conditions(flag: &Flag) -> Vec<&Ref> {
    match flag {
        Flag::Condition(c) => vec![c],
        Flag::Conditions(on, off) => vec![on, off],
        Flag::Literals(..) => Vec::new(),
    }
}

struct Check<'a> {
    layout: &'a Layout,
    program: &'a Program,
    errors: &'a mut Vec<Error>,
    /// The statements are a debugging section's, which alone may reference DEBUG-ITEM.
    debugging: bool,
    /// The most digits a numeric literal has under the program's ARITH option.
    max_digits: u32,
    /// How many inline PERFORMs the statement is inside.
    inline_performs: usize,
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
                self.inline_performs += 1;
                self.statements(body);
                self.inline_performs -= 1;
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
            Stmt::Close { files, pos } => {
                for (name, closing) in files {
                    self.file(name, *pos);
                    let keyed = self.program.files.iter().any(|f| f.name == *name && matches!(f.organization, Organization::Indexed | Organization::Relative));
                    if keyed && matches!(closing, Some(Closing::Volume | Closing::NoRewind)) {
                        self.errors.push(Error::at(*pos, format!("CLOSE {name}: REEL, UNIT and NO REWIND are not valid for an indexed or relative file")));
                    }
                }
            }
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
            Stmt::XmlParse(x) => {
                self.reference(&x.document);
                if let Some(op) = &x.encoding {
                    self.operand(op);
                }
                self.procedure(&x.procedure, x.pos);
                if let Some(t) = &x.thru {
                    self.procedure(t, x.pos);
                }
                self.statements(x.on_exception.as_deref().unwrap_or_default());
                self.statements(x.not_on_exception.as_deref().unwrap_or_default());
            }
            Stmt::JsonParse(j) => {
                self.reference(&j.source);
                self.whole_table_reference(&j.into);
                let mut named: Vec<&Ref> = j.names.iter().map(|(r, _)| r).chain(&j.suppress).chain(j.ignoring.iter().flatten()).collect();
                for (item, flag, indicator) in &j.indicating {
                    named.push(item);
                    named.extend(indicator);
                    named.extend(flag_conditions(flag));
                }
                for (item, conversion) in &j.converting {
                    named.push(item);
                    if let ParseConversion::Boolean(flag) = conversion {
                        named.extend(flag_conditions(flag));
                    }
                }
                named.into_iter().for_each(|r| self.reference_unsubscripted(r));
                if let Some(Encoding::Ccsid(op)) = &j.encoding {
                    self.operand(op);
                }
                self.statements(j.on_exception.as_deref().unwrap_or_default());
                self.statements(j.not_on_exception.as_deref().unwrap_or_default());
            }
            Stmt::XmlGenerate(x) => {
                for r in [&x.receiver, &x.from].into_iter().chain(&x.count) {
                    self.reference(r);
                }
                let named = x.names.iter().map(|(r, _)| r).chain(x.types.iter().map(|(r, _)| r));
                let suppressed = x.suppress.iter().filter_map(|s| if let Suppression::Item { item, .. } = s { Some(item) } else { None });
                named.chain(suppressed).for_each(|r| self.reference_unsubscripted(r));
                for op in [&x.encoding, &x.namespace, &x.prefix].into_iter().flatten() {
                    self.operand(op);
                }
                self.statements(x.on_exception.as_deref().unwrap_or_default());
                self.statements(x.not_on_exception.as_deref().unwrap_or_default());
            }
            Stmt::JsonGenerate(g) => {
                for r in [&g.receiver].into_iter().chain(&g.count) {
                    self.reference(r);
                }
                self.whole_table_reference(&g.from);
                let mut named: Vec<&Ref> = g.names.iter().map(|(r, _)| r).collect();
                for s in &g.suppress {
                    if let Suppression::Item { item, .. } = s {
                        named.push(item);
                    }
                }
                for (item, conversion) in &g.converting {
                    named.push(item);
                    if let JsonConversion::Boolean(Marker::Condition(c)) = conversion {
                        named.push(c);
                    }
                }
                for i in &g.indicating {
                    named.push(&i.item);
                    named.extend(&i.indicator);
                    if let Marker::Condition(c) = &i.marker {
                        named.push(c);
                    }
                }
                named.into_iter().for_each(|r| self.reference_unsubscripted(r));
                if let Some(Encoding::Ccsid(op)) = &g.encoding {
                    self.operand(op);
                }
                self.statements(g.on_exception.as_deref().unwrap_or_default());
                self.statements(g.not_on_exception.as_deref().unwrap_or_default());
            }
            Stmt::Sorting(s) => self.sorting(s),
            // Language Reference SC27-8713-03, p. 344.
            Stmt::Exit { kind: kind @ (ExitKind::Perform | ExitKind::PerformCycle), pos } if self.inline_performs == 0 => {
                let exit = if *kind == ExitKind::Perform { "EXIT PERFORM" } else { "EXIT PERFORM CYCLE" };
                self.errors.push(Error::at(*pos, format!("{exit} must be inside an inline PERFORM")));
            }
            Stmt::Goback { .. } | Stmt::StopRun { .. } | Stmt::ExitProgram { .. } | Stmt::ExitMethod { .. } | Stmt::Continue | Stmt::Exit { .. } | Stmt::NextSentence | Stmt::SentenceEnd => {}
            Stmt::Corresponding(_) => unreachable!("CORRESPONDING is expanded before Check"),
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
        if oo::special_register(self.layout, r) || markup::xml_register(self.layout, r) {
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
            self.dli_block(block);
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

    /// JSON GENERATE's and JSON PARSE's own item, which may name a whole table by leaving out its
    /// last subscript (Programming Guide SC27-8714-03, pp. 612-614, 619-620).
    fn whole_table_reference(&mut self, r: &Ref) {
        if let Ok(layout::Resolved::Item(i)) = self.layout.resolve(&r.name, &r.qualifiers, r.pos)
            && self.layout.items[i].table
            && self.layout.items[i].dims.len() == r.subscripts.len() + 1
        {
            r.subscripts.iter().for_each(|e| self.expr(e));
            return;
        }
        self.reference(r);
    }

    /// An EXEC DLI command and its options against IMS's table, and each qualification's form.
    fn dli_block(&mut self, block: &ExecBlock) {
        let Some(command) = syntax::dli::find(&block.command) else {
            self.errors.push(Error::at(block.pos, format!("EXEC DLI {} is not an EXEC DLI command", block.command)));
            return;
        };
        for (name, arg) in &block.options {
            if command.options.is_some_and(|options| !options.contains(&name.as_str())) {
                self.errors.push(Error::at(block.pos, format!("EXEC DLI {}: {name} is not one of its options", command.name)));
            } else if let (true, Some(ExecArg::Text(t))) = (name == "WHERE", arg)
                && let Err(why) = syntax::dli::qualification(t)
            {
                self.errors.push(Error::at(block.pos, format!("EXEC DLI {} WHERE({t}): {why}", command.name)));
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
            Operand::Literal(Literal::Number(t)) if literal_fixed(t).is_none() || literal_digits(t) > self.max_digits as usize => {
                self.errors.push(Error::at(Pos::default(), format!("the literal {t} has more than {} digits", self.max_digits.min(31))));
            }
            Operand::Literal(_) => {}
            Operand::Function(f) => {
                if !FUNCTIONS.contains(&f.name.as_str()) && !rt::intrinsic::FUNCTIONS.contains(&f.name.as_str()) {
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
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::time::Duration;

    #[test]
    fn source_date_epoch_decides_the_compile_time_and_the_clock_stands_in_for_it() {
        let clock = Duration::from_millis(1_790_510_400_428);
        let from = |epoch: Option<&str>| compile_time_from(epoch.map(OsStr::new), clock);
        assert_eq!(from(None), Ok(CompileTime { seconds: 1_790_510_400, hundredths: 42, source: TimeSource::Clock }));
        assert_eq!(from(Some("315532800")), Ok(CompileTime { seconds: 315_532_800, hundredths: 0, source: TimeSource::SourceDateEpoch }));
        assert_eq!(from(Some("253402300799")).map(|t| t.seconds), Ok(CompileTime::LATEST));
        for bad in ["", "-1", "+5", "1.5", " 7", "253402300800"] {
            assert!(from(Some(bad)).unwrap_err().starts_with(&format!("SOURCE_DATE_EPOCH={bad}: ")), "{bad}");
        }
    }
}
