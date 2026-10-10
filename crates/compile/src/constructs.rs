//! The statement kinds, data usages and options a compiled program holds, which decide the
//! assumptions of the register a run of it names (`numeric::governs`). Both executors take them
//! from here, and a load module carries them.

use crate::Compiled;
use numeric::governs::{Facts, OptionFact, Statement as S, Usage as U};
use rt::storage::Kind;
use syntax::ast::{
    Advancing, Cond, Encoding, ExecArg, ExecKind, Expr, InvokeMethod, Literal, Loop, Object, Operand, Organization, Program, Ref, ScreenAt, ScreenPhrases, SetStmt, Sorting, Stmt, Subject,
    Varying,
};
use syntax::ast::{BinOp, Delimiter};

pub fn of(c: &Compiled) -> Facts {
    let program = &c.program;
    let mut facts = Facts::of_options(&c.options, c.ssrange, !program.options.is_empty());
    data(c, &mut facts);
    let mut walk = Walk { program, facts };
    for p in &program.paragraphs {
        crate::oo::each(&p.statements, &mut |s| walk.statement(s));
    }
    let mut facts = walk.facts;
    let declaratives = &program.declaratives;
    if !declaratives.errors.is_empty() {
        facts.statement(S::UseProcedure);
    }
    if !declaratives.debugging.is_empty() || program.environment.debugging_mode {
        facts.statement(S::Debugging);
    }
    if !program.report_writer.reports.is_empty() {
        facts.statement(S::ReportWriter);
    }
    if program.function.is_some() {
        facts.statement(S::UserFunction);
    }
    if program.oo.as_deref().is_some_and(|o| !matches!(o.unit, syntax::ast::OoUnit::Program)) {
        facts.statement(S::Invoke);
    }
    if program.sources.len() > 1 {
        facts.statement(S::CopyMember);
    }
    if !program.screens.is_empty() {
        facts.statement(S::Screen);
    }
    facts
}

/// The usages of the program's items, its files' organizations and its data clauses.
fn data(c: &Compiled, facts: &mut Facts) {
    for item in &c.layout.items {
        let usage = match item.kind {
            Kind::Zoned { .. } => U::Zoned,
            Kind::Packed { .. } => U::Packed,
            Kind::Binary { native, .. } => {
                if native != numeric::Native::No {
                    facts.usage(U::NativeBinary);
                }
                U::Binary
            }
            Kind::Float(_) => U::Float,
            Kind::National => U::National,
            Kind::Dbcs { .. } => U::Dbcs,
            Kind::NumericEdited { .. } => U::NumericEdited,
            Kind::ObjectReference => U::ObjectReference,
            Kind::ProgramPointer => U::ProgramPointer,
            Kind::Group | Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::Pointer | Kind::Index => continue,
        };
        facts.usage(usage);
    }
    let program = &c.program;
    let containers = program.containers.iter();
    for file in program.files.iter().chain(containers.clone().flat_map(|k| &k.files)) {
        match file.organization {
            Organization::Indexed => facts.usage(U::IndexedFile),
            Organization::Relative => facts.usage(U::RelativeFile),
            Organization::Sequential | Organization::LineSequential => {}
        }
        if file.linage.is_some() {
            facts.usage(U::LinageFile);
        }
        if file.external {
            facts.usage(U::External);
        }
        if file.global || file.declared_in.is_some() {
            facts.usage(U::Global);
        }
    }
    let records = program.files.iter().flat_map(|f| &f.records);
    let containers = containers.flat_map(|k| k.working_storage.iter().chain(&k.local_storage).chain(&k.linkage).chain(k.files.iter().flat_map(|f| &f.records)));
    for entry in program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(records).chain(containers.clone()) {
        if entry.sync {
            facts.usage(U::Synchronized);
        }
        if entry.depending_on.is_some() {
            facts.usage(U::OccursDepending);
        }
        if entry.external {
            facts.usage(U::External);
        }
        if entry.global {
            facts.usage(U::Global);
        }
    }
    if containers.count() > 0 {
        facts.usage(U::Global);
    }
    let environment = &program.environment;
    if !environment.alphabets.is_empty() || environment.collating_sequence.is_some() {
        facts.usage(U::Alphabet);
    }
    if environment.decimal_point_comma {
        facts.usage(U::DecimalComma);
    }
    if !environment.switches.is_empty() {
        facts.usage(U::Upsi);
    }
}

struct Walk<'p> {
    program: &'p Program,
    facts: Facts,
}

impl Walk<'_> {
    fn statement(&mut self, s: &Stmt) {
        let f = &mut self.facts;
        match s {
            Stmt::Move { from, to, .. } => {
                f.statement(S::Move);
                self.operand(from);
                self.refs(to);
            }
            Stmt::Compute { targets, expr, .. } => {
                f.statement(S::Arithmetic);
                targets.iter().for_each(|t| self.reference(&t.r));
                self.expr(expr);
            }
            Stmt::Arith(a) => {
                f.statement(S::Arithmetic);
                for (t, e) in &a.computations {
                    self.reference(&t.r);
                    self.expr(e);
                }
                if let Some((t, q, r)) = &a.remainder {
                    self.reference(&t.r);
                    self.expr(q);
                    self.expr(r);
                }
            }
            Stmt::Corresponding(c) => {
                f.statement(S::Corresponding);
                f.statement(if c.verb == syntax::ast::CorrespondingVerb::Move { S::Move } else { S::Arithmetic });
                self.reference(&c.from);
                self.reference(&c.to);
            }
            Stmt::If { cond, .. } => self.cond(cond),
            Stmt::PerformInline { repeat, .. } | Stmt::PerformProc { repeat, .. } => {
                f.statement(S::Perform);
                self.repeat(repeat);
            }
            Stmt::Evaluate { subjects, whens, .. } => {
                f.statement(S::Condition);
                for subject in subjects {
                    match subject {
                        Subject::Bool(_) => {}
                        Subject::Expr(e) => self.expr(e),
                        Subject::Cond(c) => self.cond(c),
                    }
                }
                for object in whens.iter().flat_map(|w| w.alternatives.iter().flatten()) {
                    match object {
                        Object::Any | Object::Bool(_) => {}
                        Object::Cond(c) => self.cond(c),
                        Object::Value { from, thru, .. } => {
                            self.expr(from);
                            thru.iter().for_each(|e| self.expr(e));
                        }
                    }
                }
            }
            Stmt::Display { items, screen, .. } => {
                f.statement(S::Display);
                items.iter().for_each(|o| self.operand(o));
                self.screen(screen.as_deref());
            }
            Stmt::Accept { target, screen, .. } => {
                f.statement(S::Accept);
                self.reference(target);
                self.screen(screen.as_deref());
            }
            Stmt::Open { .. } | Stmt::Close { .. } | Stmt::Delete { .. } | Stmt::DeleteFile { .. } => f.statement(S::FileIo),
            Stmt::Read(r) => {
                f.statement(S::FileIo);
                r.into.iter().chain(&r.key).for_each(|x| self.reference(x));
            }
            Stmt::Write { record, from, advancing, .. } => {
                f.statement(S::FileIo);
                if let Some(advancing) = advancing {
                    f.statement(S::WriteAdvancing);
                    if let Advancing::Lines { count, .. } = advancing {
                        self.expr(count);
                    }
                }
                self.reference(record);
                from.iter().for_each(|o| self.operand(o));
            }
            Stmt::Rewrite { record, from, .. } => {
                f.statement(S::FileIo);
                self.reference(record);
                from.iter().for_each(|o| self.operand(o));
            }
            Stmt::Start { key, .. } => {
                f.statement(S::FileIo);
                key.iter().for_each(|(_, r)| self.reference(r));
            }
            Stmt::Initialize { targets, with, .. } => {
                f.statement(S::Initialize);
                self.refs(targets);
                with.iter().flat_map(|w| &w.replacing).for_each(|(_, o)| self.operand(o));
            }
            Stmt::GoTo { target: None, .. } | Stmt::Alter { .. } => f.statement(S::Alter),
            Stmt::GoToDepending { on, .. } => self.reference(on),
            Stmt::Entry { .. } => f.statement(S::Entry),
            Stmt::Call(c) => {
                f.statement(S::Call);
                self.operand(&c.target);
                c.using.iter().filter_map(|a| a.value.as_ref()).for_each(|o| self.operand(o));
                c.returning.iter().for_each(|r| self.reference(r));
            }
            Stmt::Cancel { targets, .. } => {
                f.statement(S::Cancel);
                targets.iter().for_each(|o| self.operand(o));
            }
            Stmt::Set { set, .. } => {
                f.statement(S::Set);
                match set {
                    SetStmt::ConditionTrue(refs) | SetStmt::ConditionFalse(refs) => self.refs(refs),
                    SetStmt::To { targets, value } | SetStmt::Entry { targets, entry: value } | SetStmt::AddressOf { targets, value } => {
                        self.refs(targets);
                        self.operand(value);
                    }
                    SetStmt::UpDown { targets, by, .. } => {
                        self.refs(targets);
                        self.expr(by);
                    }
                    SetStmt::Switches(groups) => groups.iter().for_each(|(refs, _)| self.refs(refs)),
                }
            }
            Stmt::String(st) => {
                for (o, d) in &st.sources {
                    self.operand(o);
                    if let Delimiter::By(d) = d {
                        self.operand(d);
                    }
                }
                self.reference(&st.into);
                st.pointer.iter().for_each(|r| self.reference(r));
            }
            Stmt::Unstring(u) => {
                self.operand(&u.source);
                u.delimiters.iter().for_each(|(_, o)| self.operand(o));
                for into in &u.into {
                    [Some(&into.target), into.delimiter_in.as_ref(), into.count_in.as_ref()].into_iter().flatten().for_each(|r| self.reference(r));
                }
                u.pointer.iter().chain(&u.tallying).for_each(|r| self.reference(r));
            }
            Stmt::Inspect(i) => {
                f.statement(S::Inspect);
                self.operand(&i.target);
                for p in i.tallying.iter().chain(&i.replacing) {
                    p.pattern.iter().chain(&p.by).chain(p.bounds.iter().map(|b| &b.value)).for_each(|o| self.operand(o));
                    p.counter.iter().for_each(|r| self.reference(r));
                }
                if let Some((from, to, bounds)) = &i.converting {
                    [from, to].into_iter().chain(bounds.iter().map(|b| &b.value)).for_each(|o| self.operand(o));
                }
            }
            Stmt::Search(se) => {
                f.statement(S::Condition);
                self.reference(&se.table);
                se.varying.iter().for_each(|r| self.reference(r));
                se.whens.iter().for_each(|(c, _)| self.cond(c));
            }
            Stmt::Exec(b) => {
                match b.kind {
                    ExecKind::Sql => f.statement(S::Sql),
                    ExecKind::Cics => f.statement(S::Cics),
                    ExecKind::Dli => f.statement(S::Dli),
                    ExecKind::Other => {}
                }
                for (_, arg) in &b.options {
                    if let Some(ExecArg::Operand(o)) = arg {
                        self.operand(o);
                    }
                }
                self.refs(&b.host_variables);
            }
            Stmt::Report(_) => f.statement(S::ReportWriter),
            Stmt::Invoke(i) => {
                f.statement(S::Invoke);
                self.reference(&i.target);
                if let InvokeMethod::Identifier(r) = &i.method {
                    self.reference(r);
                }
                i.using.iter().for_each(|o| self.operand(o));
                i.returning.iter().for_each(|r| self.reference(r));
            }
            Stmt::JsonGenerate(j) => {
                f.statement(S::Json);
                self.encoding(j.encoding.as_ref());
            }
            Stmt::JsonParse(j) => {
                f.statement(S::Json);
                self.encoding(j.encoding.as_ref());
            }
            Stmt::XmlGenerate(x) => {
                f.statement(S::Xml);
                x.encoding.iter().chain(&x.namespace).chain(&x.prefix).for_each(|o| self.operand(o));
            }
            Stmt::XmlParse(x) => {
                f.statement(S::Xml);
                x.encoding.iter().for_each(|o| self.operand(o));
            }
            Stmt::Sorting(so) => match &**so {
                Sorting::Sort(sort) => {
                    let file = self.program.files.iter().any(|f| f.sort && f.name.eq_ignore_ascii_case(&sort.subject.name));
                    self.facts.statement(match (sort.merge, file) {
                        (true, _) => S::Merge,
                        (false, true) => S::Sort,
                        (false, false) => S::TableSort,
                    });
                    if sort.collating.is_some() {
                        self.facts.usage(U::Alphabet);
                    }
                    sort.keys.iter().for_each(|(_, r)| self.reference(r));
                }
                Sorting::Release { record, from, .. } => {
                    self.facts.statement(S::Sort);
                    self.reference(record);
                    from.iter().for_each(|o| self.operand(o));
                }
                Sorting::Return { into, .. } => {
                    self.facts.statement(S::Sort);
                    into.iter().for_each(|r| self.reference(r));
                }
            },
            Stmt::StopRun { .. } => f.statement(S::Stop),
            Stmt::GoTo { target: Some(_), .. }
            | Stmt::Goback { .. }
            | Stmt::ExitProgram { .. }
            | Stmt::NextSentence
            | Stmt::SentenceEnd
            | Stmt::ExitMethod { .. }
            | Stmt::Continue { .. }
            | Stmt::Hole { .. }
            | Stmt::Exit { .. } => {}
        }
    }

    fn repeat(&mut self, repeat: &Loop) {
        match repeat {
            Loop::Once | Loop::Forever => {}
            Loop::Times(e) => self.expr(e),
            Loop::Until { cond, .. } => self.cond(cond),
            Loop::Varying { varying, after, .. } => std::iter::once(&**varying).chain(after).for_each(|v| self.varying(v)),
        }
    }

    fn varying(&mut self, v: &Varying) {
        self.facts.statement(S::Arithmetic);
        self.reference(&v.var);
        self.expr(&v.from);
        self.expr(&v.by);
        self.cond(&v.until);
    }

    fn screen(&mut self, screen: Option<&ScreenPhrases>) {
        let Some(screen) = screen else { return };
        self.facts.statement(S::Screen);
        match &screen.at {
            Some(ScreenAt::Combined(o)) => self.operand(o),
            Some(ScreenAt::LineColumn { line, column }) => line.iter().chain(column).for_each(|o| self.operand(o)),
            None => {}
        }
    }

    fn encoding(&mut self, encoding: Option<&Encoding>) {
        if let Some(Encoding::Ccsid(o)) = encoding {
            self.operand(o);
        }
    }

    fn cond(&mut self, c: &Cond) {
        self.facts.statement(S::Condition);
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

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Operand(o) => self.operand(o),
            Expr::Neg(inner) => {
                self.facts.statement(S::Arithmetic);
                self.expr(inner);
            }
            Expr::Bin(a, op, b) => {
                self.facts.statement(S::Arithmetic);
                if *op == BinOp::Pow {
                    self.facts.statement(S::Exponentiation);
                }
                self.expr(a);
                self.expr(b);
            }
        }
    }

    fn operand(&mut self, o: &Operand) {
        match o {
            Operand::Ref(r) | Operand::LengthOf(r) | Operand::AddressOf(r) => self.reference(r),
            Operand::Literal(l) => self.literal(l),
            Operand::Function(call) => {
                self.facts.statement(if self.program.intrinsic(&call.name) { S::Function } else { S::UserFunction });
                call.args.iter().for_each(|a| self.expr(a));
                if let Some(m) = &call.refmod {
                    self.expr(&m.start);
                    m.length.iter().for_each(|l| self.expr(l));
                }
            }
        }
    }

    fn literal(&mut self, l: &Literal) {
        match l {
            Literal::National(_) => self.facts.usage(U::National),
            Literal::Dbcs(_) => self.facts.usage(U::Dbcs),
            Literal::All(inner) => self.literal(inner),
            Literal::Alnum(_) | Literal::Hex(_) | Literal::Number(_) | Literal::Figurative(_) => {}
        }
    }

    fn refs(&mut self, refs: &[Ref]) {
        refs.iter().for_each(|r| self.reference(r));
    }

    fn reference(&mut self, r: &Ref) {
        r.subscripts.iter().for_each(|e| self.expr(e));
        if let Some(m) = &r.refmod {
            self.expr(&m.start);
            m.length.iter().for_each(|l| self.expr(l));
        }
    }
}

/// A run's own facts beside its programs': how it was made.
pub fn of_run(cics: bool, job: bool, parm: bool, statement_limit: bool) -> Facts {
    let mut facts = Facts::default();
    for (on, fact) in [(cics, OptionFact::CicsTask), (job, OptionFact::JobStep), (parm, OptionFact::Parm), (statement_limit, OptionFact::StatementLimit)] {
        if on {
            facts.option(fact);
        }
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use numeric::governs::Trigger;

    fn facts_of(card: &str, data: &[&str], procedure: &[&str]) -> Facts {
        let lines = ["IDENTIFICATION DIVISION.", "PROGRAM-ID. FACTS.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL.", "    SELECT KEYED ASSIGN TO KEYED ORGANIZATION INDEXED", "        RECORD KEY K-KEY.", "DATA DIVISION.", "FILE SECTION.", "FD  KEYED.", "01  K-REC.", "    05 K-KEY PIC X(4).", "WORKING-STORAGE SECTION."]
            .into_iter()
            .chain(data.iter().copied())
            .chain(["PROCEDURE DIVISION."])
            .chain(procedure.iter().copied())
            .chain(["    GOBACK."]);
        let source: String = std::iter::once(card.to_owned()).chain(lines.map(|l| format!("       {l}"))).map(|l| l + "\n").collect();
        of(&crate::compile(syntax::parse(&source).unwrap(), &[]).unwrap_or_else(|e| panic!("{e:?}")))
    }

    #[test]
    fn a_program_holds_the_statements_usages_and_options_it_is_written_with() {
        let data = ["01  P PIC S9(5) COMP-3 VALUE 1.", "01  F COMP-2.", "01  T.", "    05 E PIC X OCCURS 3.", "01  N PIC N(2) VALUE N'AB'."];
        let procedure = ["    COMPUTE P = P ** 2", "    IF FUNCTION LENGTH(N) > 0 DISPLAY E(P + 1) END-IF", "    SORT E ASCENDING", "    OPEN INPUT KEYED"];
        let facts = facts_of("       CBL TRUNC(OPT)", &data, &procedure);
        let statements: Vec<_> = facts.statements().collect();
        assert_eq!(statements, [S::Arithmetic, S::Exponentiation, S::Display, S::Condition, S::TableSort, S::FileIo, S::Function]);
        for usage in [U::Packed, U::Float, U::National, U::IndexedFile] {
            assert!(facts.has(Trigger::Usage(usage)), "{usage:?}");
        }
        assert!(!facts.has(Trigger::Usage(U::Binary)));
        assert_eq!(facts.options().collect::<Vec<_>>(), [OptionFact::TruncOpt, OptionFact::Cards]);
        assert!(!facts.has(Trigger::Statement(S::Sort)));
    }

    #[test]
    fn a_run_holds_how_it_was_made() {
        let facts = of_run(true, false, true, false);
        assert_eq!(facts.options().collect::<Vec<_>>(), [OptionFact::CicsTask, OptionFact::Parm]);
        assert_eq!(facts.statements().count() + facts.usages().count(), 0);
    }
}
