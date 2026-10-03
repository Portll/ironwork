use super::*;
use crate::testing::{check_lowering, encoded, line};
use rt::lir::{
    ArithPlan, Base, CallArg, CallTarget, Chars, Collating, Comparand, Cond as LirCond, Const, DisplayItem, Image, InitField, InitValue, IntExpr, LeService,
    MethodName, Mode, MovePlan, NationalFrom, NumericFrom, Odo, Op, Operand as LirOperand, Place, Program, Receiver, SenderCheck, SignTest, StorePlan, Terminator,
};
use rt::abend::Ending;
use rt::module::codec::decode_all;

mod cics;
mod corpus;
mod markup;
mod report;
mod sort;
mod sql;

fn program(options: &str, data: &str, procedure: &str) -> String {
    let card = if options.is_empty() { String::new() } else { format!("       CBL {options}\n") };
    format!("{card}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{procedure}")
}

fn compiled(source: &str) -> Compiled {
    compiled_with(source, &[])
}

fn compiled_with(source: &str, flags: &[&str]) -> Compiled {
    let flags: Vec<String> = flags.iter().map(|f| f.to_string()).collect();
    crate::compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &flags).unwrap_or_else(|e| panic!("{e:?}"))
}

fn lowered(source: &str) -> Program {
    let p = lower(&compiled(source)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(round_trip(&p), p);
    p
}

/// The program through the load-module codec, checked to encode again to the same bytes.
fn round_trip(p: &Program) -> Program {
    let (bytes, strings) = encoded(p);
    let decoded = decode_all::<Program>("LIR", &bytes, &strings).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(encoded(&decoded), (bytes, strings));
    decoded
}

fn paragraph(p: &Program, name: &str) -> usize {
    p.paragraphs.iter().position(|q| p.symbols[q.name as usize] == name).unwrap()
}

/// The terminator control reaches from a paragraph's entry through plain jumps and straight code.
fn end_of(p: &Program, name: &str) -> Terminator {
    let mut b = p.paragraphs[paragraph(p, name)].entry as usize;
    loop {
        match &p.blocks[b].end {
            Terminator::Jump(t) if p.paragraphs.iter().all(|q| q.entry as usize != *t as usize) => b = *t as usize,
            other => return other.clone(),
        }
    }
}

fn ops(p: &Program) -> impl Iterator<Item = &Op> {
    p.blocks.iter().flat_map(|b| &b.ops)
}

fn moves(p: &Program) -> Vec<MovePlan> {
    ops(p).filter_map(|op| if let Op::Move { plan, .. } | Op::Set { plan, .. } = op { Some(*plan) } else { None }).collect()
}

fn place_named<'p>(p: &'p Program, name: &str) -> Vec<&'p Place> {
    p.places.iter().filter(|q| p.symbols[q.name as usize] == name).collect()
}

#[test]
fn a_go_to_out_of_a_perform_range_stays_a_transfer_and_the_range_s_end_keeps_its_check() {
    let p = lowered(&program(
        "",
        "       01  K PIC 9 VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM A THRU C"),
            &line("STOP RUN."),
            "       A.\n",
            &line("ADD 1 TO K."),
            "       B.\n",
            &line("GO TO D."),
            "       C.\n",
            &line("DISPLAY 'C'."),
            "       D.\n",
            &line("DISPLAY 'D' K."),
        ]
        .concat(),
    ));
    let (a, c, d) = (paragraph(&p, "A") as u32, paragraph(&p, "C") as u32, paragraph(&p, "D") as u32);
    assert_eq!(p.ranges, [lir::Range { first: a, last: c, kind: lir::RangeKind::Perform }]);
    assert_eq!(end_of(&p, "B"), Terminator::GoTo(d));
    assert_eq!(end_of(&p, "C"), Terminator::ParagraphEnd { next: d });
    assert_eq!(end_of(&p, "A"), Terminator::Jump(p.paragraphs[a as usize + 1].entry));
    assert_eq!(end_of(&p, "D"), Terminator::ParagraphEnd { next: d + 1 });
    let enter = p.blocks.iter().find_map(|b| if let Terminator::PerformEnter { range, ret, resume } = b.end { Some((range, ret, resume)) } else { None });
    let (range, ret, resume) = enter.unwrap();
    assert_eq!(range, 0);
    assert_eq!(p.blocks[ret as usize].ops, [Op::Unnest(1)]);
    let resume = resume.unwrap();
    assert_eq!((resume.para, &p.blocks[ret as usize].end), (paragraph(&p, "MAIN-LINE") as u32, &Terminator::Jump(resume.block)));
    assert!(matches!(p.blocks[resume.block as usize].end, Terminator::End(Ending::StopRun)));
    let abandoned: Vec<bool> = p.paragraphs.iter().map(|q| q.abandoned.is_some()).collect();
    assert_eq!(abandoned, [false, false, false, true, false]);
    let text = p.abends[p.paragraphs[c as usize].abandoned.unwrap() as usize].clone();
    assert!(symbol(&p, text.message).starts_with("control passed the end of C, which is armed to return to a PERFORM that control left by GO TO"));
    assert_eq!(text.at.map(|at| p.debug.positions[at as usize].line), Some(14));
}

#[test]
fn a_perform_resumes_only_when_it_runs_once_and_is_a_statement_of_its_paragraph() {
    let p = lowered(&program(
        "",
        "       01  K PIC 9 VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM P"),
            &line("IF K = 0 PERFORM P END-IF"),
            &line("PERFORM P 2 TIMES"),
            &line("PERFORM P UNTIL K > 0"),
            &line("PERFORM P."),
            &line("STOP RUN."),
            "       P.\n",
            &line("ADD 1 TO K."),
        ]
        .concat(),
    ));
    let resumes: Vec<Option<lir::Resume>> = p.blocks.iter().filter_map(|b| if let Terminator::PerformEnter { resume, .. } = b.end { Some(resume) } else { None }).collect();
    assert_eq!(resumes.len(), 5);
    assert_eq!(resumes.iter().flatten().map(|r| r.para).collect::<Vec<_>>(), [0, 0], "{resumes:?}");
    assert_eq!(p.ranges.len(), 1);
    assert!(p.paragraphs[paragraph(&p, "P")].abandoned.is_some());
}

#[test]
fn overlapping_ranges_keep_a_check_at_each_range_s_end() {
    let p = lowered(&program(
        "",
        "",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM A THRU C"),
            &line("STOP RUN."),
            "       A.\n",
            &line("PERFORM B THRU D."),
            "       B.\n",
            &line("DISPLAY 'B'."),
            "       C.\n",
            &line("DISPLAY 'C'."),
            "       D.\n",
            &line("DISPLAY 'D'."),
        ]
        .concat(),
    ));
    let (b, c, d) = (paragraph(&p, "B") as u32, paragraph(&p, "C") as u32, paragraph(&p, "D") as u32);
    assert_eq!(p.ranges.len(), 2);
    assert_eq!(end_of(&p, "B"), Terminator::Jump(p.paragraphs[c as usize].entry));
    assert_eq!(end_of(&p, "C"), Terminator::ParagraphEnd { next: d });
    assert_eq!(end_of(&p, "D"), Terminator::ParagraphEnd { next: d + 1 });
    assert!(b < c);
}

/// The parser reads `EXIT SECTION` at the start of a sentence as a section header, so the
/// statement is put into the parsed program in place of an EXIT PARAGRAPH.
#[test]
fn exit_section_leaves_for_the_paragraph_after_the_section() {
    let source = program(
        "",
        "       01  K PIC 9 VALUE 0.\n",
        &[
            "       MAIN SECTION.\n",
            &line("PERFORM WORK"),
            &line("STOP RUN."),
            "       WORK SECTION.\n",
            "       STEP-1.\n",
            &line("ADD 1 TO K"),
            &line("EXIT PARAGRAPH."),
            "       NEVER.\n",
            &line("ADD 5 TO K."),
            "       OTHER-S SECTION.\n",
            &line("DISPLAY K."),
        ]
        .concat(),
    );
    let mut parsed = syntax::parse(&source).unwrap();
    for s in parsed.paragraphs.iter_mut().flat_map(|p| p.statements.iter_mut()) {
        if let ast::Stmt::Exit { kind: kind @ ast::ExitKind::Paragraph, .. } = s {
            *kind = ast::ExitKind::Section;
        }
    }
    let p = lower(&crate::compile(parsed, &[]).unwrap()).unwrap();
    let other = paragraph(&p, "OTHER-S") as u32;
    let work = paragraph(&p, "WORK") as u32;
    assert_eq!(p.ranges[0], lir::Range { first: work, last: other - 1, kind: lir::RangeKind::Perform });
    assert_eq!(p.paragraphs[work as usize].section_end, other - 1);
    assert_eq!(end_of(&p, "STEP-1"), Terminator::ParagraphEnd { next: other });
}

#[test]
fn subscripts_reference_modification_and_odo_carry_checks_only_under_ssrange() {
    let data = "       01  I PIC 9(2) VALUE 2.\n       01  T.\n           05 N PIC 9.\n           05 E PIC X(4) OCCURS 1 TO 5 DEPENDING ON N.\n       01  W PIC X(10).\n";
    let body = &[line("MOVE E(I)(2:I) TO W"), line("MOVE T TO W"), line("GOBACK.")].concat();
    for (options, ssrange) in [("SSRANGE", true), ("", false)] {
        let p = lowered(&program(options, data, body));
        assert_eq!(p.options.ssrange, ssrange);
        let e = place_named(&p, "E")[0];
        assert_eq!((e.base, e.offset, e.len), (Base::Program, 9, 4));
        assert_eq!(e.kind, rt::storage::Kind::Alnum { justified: false });
        assert_eq!(e.subscripts.len(), 1);
        assert_eq!(e.subscripts[0].stride, 4);
        assert_eq!(e.subscripts[0].check, ssrange.then_some(5));
        let refmod = e.refmod.clone().unwrap();
        assert_eq!(refmod.start, IntExpr::Const(2));
        assert!(matches!(refmod.length, Some(IntExpr::Item(_))));
        assert_eq!(refmod.check, ssrange);
        let t = place_named(&p, "T");
        let odo = t.iter().find_map(|q| q.odo.first().cloned()).unwrap();
        assert_eq!((odo.max, odo.element, odo.check), (5, 4, ssrange));
    }
}

#[test]
fn a_receiving_group_holding_its_own_odo_object_is_at_its_maximum_length() {
    let data = "       01  REC.\n           05 CNT PIC 9.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n       01  W PIC X(6).\n";
    let p = lowered(&program("", data, &[line("MOVE W TO REC"), line("MOVE REC TO W"), line("GOBACK.")].concat()));
    let rec = place_named(&p, "REC");
    assert_eq!(rec.len(), 2);
    assert_eq!(rec.iter().filter(|q| q.odo.is_empty()).count(), 1);
}

#[test]
fn a_variably_located_item_moves_back_by_the_tables_ahead_of_it_and_its_group_counts_both() {
    let data = concat!(
        "       01  REC.\n           05 CNT PIC 9.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n",
        "           05 LATER PIC X.\n           05 MORE PIC X OCCURS 1 TO 3 DEPENDING ON CNT.\n           05 LAST PIC X.\n       01  W PIC X(11).\n",
    );
    let body = ["MOVE LATER TO W", "MOVE MORE (2) TO W", "MOVE LAST TO W", "MOVE REC TO W", "MOVE W TO REC", "GOBACK."].map(line).concat();
    let p = lowered(&program("", data, &body));
    let tables = |odos: &[Odo]| odos.iter().map(|o| (o.max, o.element)).collect::<Vec<_>>();
    let shape = |name: &str| place_named(&p, name).iter().map(|q| (tables(&q.moved), q.subscripts.len(), tables(&q.odo))).collect::<Vec<_>>();
    assert_eq!(shape("LATER"), [(vec![(5, 1)], 0, vec![])]);
    assert_eq!(shape("MORE"), [(vec![(5, 1)], 1, vec![])]);
    assert_eq!(shape("LAST"), [(vec![(5, 1), (3, 1)], 0, vec![])]);
    // A sender REC is as long as both counts make it; as a receiver holding its objects, its maximum.
    assert_eq!(shape("REC"), [(vec![], 0, vec![(5, 1), (3, 1)]), (vec![], 0, vec![])]);
}

#[test]
fn compute_rounded_with_on_size_error_selects_its_handler() {
    let p = lowered(&program(
        "",
        "       01  A PIC S9(3)V99 COMP-3 VALUE 12.5.\n       01  B PIC 9(3)V9.\n",
        &[line("COMPUTE B ROUNDED = A * 3 / 7"), line("    ON SIZE ERROR DISPLAY 'SIZE'"), line("    NOT ON SIZE ERROR DISPLAY 'OK'"), line("END-COMPUTE"), line("GOBACK.")].concat(),
    ));
    let plan = &p.plans.arith[0];
    assert!(plan.handled);
    assert_eq!(plan.dmax, 2);
    assert_eq!(plan.steps[0].mode, Mode::Fixed);
    assert!(plan.steps[0].rounded);
    assert_eq!(plan.steps[0].store, StorePlan::Zoned { digits: 4, scale: 1, signed: false, sign: None });
    assert!(plan.prepass.is_empty());
    let select = p.blocks.iter().find(|b| matches!(b.end, Terminator::Select(_))).unwrap();
    assert_eq!(select.ops.last(), Some(&Op::Arith(0)));
    let Terminator::Select(arms) = &select.end else { unreachable!() };
    let shown = |b: u32| match p.blocks[b as usize].ops.first() {
        Some(Op::Display(d)) => match p.plans.display[*d as usize].items[0] {
            DisplayItem::Text(t) => p.symbols[t as usize].clone(),
            _ => String::new(),
        },
        _ => String::new(),
    };
    assert_eq!((shown(arms[0]), shown(arms[1])), ("OK".to_owned(), "SIZE".to_owned()));
}

#[test]
fn add_to_two_subscripted_receivers_locates_both_before_any_store() {
    let p = lowered(&program(
        "SSRANGE",
        "       01  T.\n           05 V PIC 9(3) OCCURS 3.\n       01  I PIC 9 VALUE 1.\n       01  J PIC 9 VALUE 4.\n       01  X COMP-2.\n",
        &[line("ADD 1 TO V(I) V(J)"), line("COMPUTE V(I) = X + V(J)"), line("GOBACK.")].concat(),
    ));
    let ArithPlan { prepass, steps, .. } = &p.plans.arith[0];
    assert_eq!(steps.len(), 2);
    assert_eq!(prepass.len(), 4, "each receiver, and each as an operand of its own expression");
    assert_eq!(prepass[0], steps[0].target);
    assert_eq!(prepass[2], steps[1].target);
    assert_eq!(steps[0].probe, [steps[0].target]);
    let compute = &p.plans.arith[1];
    assert!(matches!(compute.steps[0].mode, Mode::Float(_)));
    assert!(compute.steps[0].probe.is_empty(), "the float test stops at X, before V(J)");
    assert_eq!(compute.prepass.len(), 2);
}

#[test]
fn a_condition_name_keeps_each_value_and_thru_pair() {
    let p = lowered(&program(
        "",
        "       01  N PIC 99 VALUE 15.\n          88 SOME VALUE 1 3 THRU 5 13 THRU 19.\n       01  C PIC X VALUE 'B'.\n          88 IS-AB VALUE 'A' 'B'.\n",
        &[line("IF SOME AND IS-AB DISPLAY 'YES' END-IF"), line("GOBACK.")].concat(),
    ));
    let names: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Name { values, how, .. } = c { Some((values.clone(), *how)) } else { None }).collect();
    assert_eq!(names.len(), 2);
    let (values, how) = &names[0];
    assert_eq!(*how, lir::Compare::Fixed);
    let number = |c: u32| match &p.consts[c as usize] {
        Const::Number(f) => f.magnitude.to_u128().unwrap(),
        other => panic!("{other:?}"),
    };
    let values: Vec<_> = values.iter().map(|&(low, high)| (number(low), high.map(number))).collect();
    assert_eq!(values, [(1, None), (3, Some(5)), (13, Some(19))]);
    assert_eq!(names[1].1, lir::Compare::Alphanumeric);
    assert!(p.conds.iter().any(|c| matches!(c, LirCond::And(..))));
}

#[test]
fn a_condition_name_whose_variable_name_repeats_tests_its_own_item() {
    let p = lowered(&program(
        "",
        "       01  G1.\n           05 F PIC X VALUE 'A'.\n              88 F-A VALUE 'A'.\n       01  G2.\n           05 F PIC X.\n",
        &[line("DISPLAY 'BEFORE'"), line("IF F-A DISPLAY 'A' END-IF"), line("GOBACK.")].concat(),
    ));
    assert!(p.blocks.iter().all(|b| !matches!(b.end, Terminator::Abend(_))));
    let tested = p.places.iter().find(|q| symbol(&p, q.name) == "F-A").unwrap();
    assert_eq!(tested.offset, 0);
}

#[test]
fn move_plans_follow_the_walkers_categories() {
    let data = concat!(
        "       01  A PIC X(4) VALUE 'AB'.\n       01  AJ PIC X(4) JUSTIFIED RIGHT.\n       01  Z PIC 9(4).\n       01  P PIC S9(5) COMP-3.\n",
        "       01  P2 PIC S9(5) COMP-3.\n       01  E PIC ZZ9.99.\n       01  D PIC 9(3)V99.\n       01  NA PIC N(3).\n       01  AE PIC XXBXX.\n",
        "       01  F COMP-2.\n       01  PTR POINTER.\n",
    );
    let body = [
        "MOVE A TO AJ",
        "MOVE A TO Z",
        "MOVE Z TO A",
        "MOVE D TO A",
        "MOVE E TO D",
        "MOVE A TO NA",
        "MOVE ZERO TO Z",
        "MOVE SPACE TO Z",
        "MOVE ALL '1' TO Z",
        "MOVE P TO P2",
        "MOVE A TO AE",
        "MOVE Z TO F",
        "MOVE A TO PTR",
        "MOVE NA TO A",
        "GOBACK.",
    ]
    .map(line)
    .concat();
    for (options, packed) in [("", NumericFrom::Value), ("NUMPROC(PFD)", NumericFrom::PackedCopy)] {
        let p = lowered(&program(options, data, &body));
        let plans = moves(&p);
        let zoned = StorePlan::Zoned { digits: 4, scale: 0, signed: false, sign: None };
        assert_eq!(plans[0], MovePlan::Alnum { image: Image::Bytes, justified: true });
        assert_eq!(plans[1], MovePlan::Numeric { from: NumericFrom::Zoned, store: zoned });
        assert_eq!(plans[2], MovePlan::Alnum { image: Image::Digits { digits: 4 }, justified: false });
        assert!(matches!(plans[3], MovePlan::Refused(_)));
        assert!(matches!(plans[4], MovePlan::Numeric { from: NumericFrom::DeEdit { digits: 5, scale: 2, .. }, .. }));
        assert_eq!(plans[5], MovePlan::National(NationalFrom::Decoded));
        assert_eq!(plans[6], MovePlan::Numeric { from: NumericFrom::Zero, store: zoned });
        assert_eq!(plans[7], MovePlan::Numeric { from: NumericFrom::Fill, store: zoned });
        assert_eq!(plans[8], MovePlan::Numeric { from: NumericFrom::Fill, store: zoned });
        assert!(matches!(plans[9], MovePlan::Numeric { from, .. } if from == packed));
        assert!(matches!(plans[10], MovePlan::AlnumEdited { image: Image::Bytes, positions: 4, .. }));
        assert!(matches!(plans[11], MovePlan::Float { from: lir::FloatFrom::Fixed, .. }));
        assert!(matches!(plans[12], MovePlan::Refused(_)));
        assert!(matches!(plans[13], MovePlan::Refused(_)));
    }
}

#[test]
fn next_sentence_goes_past_the_next_period_and_leaves_inline_performs() {
    let p = lowered(&program(
        "",
        "       01  K PIC 9 VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM 2 TIMES"),
            &line("    IF K = 0 NEXT SENTENCE END-IF"),
            &line("    DISPLAY 'NOT'"),
            &line("END-PERFORM"),
            &line("DISPLAY 'SKIPPED'."),
            &line("DISPLAY 'AFTER'"),
            &line("GOBACK."),
        ]
        .concat(),
    ));
    let unnest = p.blocks.iter().find(|b| b.ops.last() == Some(&Op::Unnest(1)) && matches!(b.end, Terminator::Jump(_)) && b.ops.len() == 1);
    let Terminator::Jump(target) = unnest.unwrap().end else { unreachable!() };
    let Some(Op::Display(d)) = p.blocks[target as usize].ops.first() else { panic!("{:?}", p.blocks[target as usize]) };
    let DisplayItem::Text(t) = p.plans.display[*d as usize].items[0] else { panic!() };
    assert!(["AFTER", "NOT"].contains(&p.symbols[t as usize].as_str()));
}

#[test]
fn perform_varying_moves_from_steps_by_and_tests_before_the_body() {
    let p = lowered(&program(
        "",
        "       01  I PIC 9(2).\n       01  S PIC 9(4) VALUE 0.\n",
        &[line("PERFORM VARYING I FROM 1 BY 2 UNTIL I > 10"), line("    ADD I TO S"), line("END-PERFORM"), line("DISPLAY S"), line("GOBACK.")].concat(),
    ));
    let all: Vec<&Op> = ops(&p).collect();
    assert_eq!(all[0], &Op::Nest);
    assert!(matches!(all[1], Op::Set { plan: MovePlan::Numeric { from: NumericFrom::Value, .. }, .. }));
    let step = all.iter().find_map(|op| if let Op::Step { plan, .. } = op { Some(*plan) } else { None }).unwrap();
    assert_eq!(step.dmax, 0);
    assert_eq!(step.store, StorePlan::Zoned { digits: 2, scale: 0, signed: false, sign: None });
}

#[test]
fn storage_is_the_image_value_clauses_leave_with_their_trunc_reports() {
    let p = lowered(&program(
        "TRUNC(OPT)",
        "       01  A PIC X(3) VALUE 'AB'.\n       01  B PIC S9(3) COMP-3 VALUE -12.\n       01  H PIC 9(2) BINARY VALUE 123.\n",
        &line("GOBACK."),
    ));
    assert_eq!(&p.storage.image[..3], &[0xC1, 0xC2, 0x40]);
    assert_eq!(&p.storage.image[8..10], &[0x01, 0x2D]);
    assert_eq!(p.storage.init_reports.len(), 1);
    assert!(p.symbols[p.storage.init_reports[0] as usize].contains("TRUNC(OPT)"));
    assert_eq!(p.storage.init_abend, None);
}

#[test]
fn every_op_and_terminator_names_a_position() {
    let p = lowered(&program("", "       01  K PIC 9.\n", &[line("MOVE 1 TO K"), line("DISPLAY K"), line("GOBACK.")].concat()));
    for (block, ids) in p.blocks.iter().zip(&p.debug.ops) {
        assert_eq!(ids.len(), block.ops.len() + 1);
    }
    let moved = p.debug.ops[0][0];
    assert_eq!(p.debug.positions[moved as usize].line, 7);
    assert!(matches!(p.blocks[0].ops[0], Op::Move { from: LirOperand::Const(_), .. }));
    verify(&p).unwrap();
}

#[test]
fn initialize_s_phrases_choose_each_field_and_what_it_is_sent() {
    let data = "       01  G.\n           05 A PIC 9 VALUE 4.\n           05 FILLER PIC X VALUE 'F'.\n           05 E PIC X/X VALUE 'AB'.\n       01  N PIC 9.\n";
    let fields = |phrase: &str| {
        let p = lowered(&program("", data, &[line(&format!("INITIALIZE G {phrase}")), line("GOBACK.")].concat()));
        let sent = |f: &InitField| match f.value {
            InitValue::Default(fill) => format!("{fill:?}"),
            InitValue::Value(c) => format!("{:?}", p.consts[c as usize]),
            InitValue::Replacing(LirOperand::Load(q)) => symbol(&p, p.places[q as usize].name).to_owned(),
            other => format!("{other:?}"),
        };
        p.plans.init[0].fields.iter().map(|f| (f.offset, sent(f), matches!(f.store, MovePlan::Alnum { .. }))).collect::<Vec<_>>()
    };
    let field = |offset: u32, sent: &str, alnum: bool| (offset, sent.to_owned(), alnum);
    assert_eq!(fields(""), [field(0, "Zero", false), field(2, "Space", false)]);
    let number = format!("{:?}", Const::Number(crate::machine::literal_fixed("4").unwrap()));
    let bytes = |text: &str| format!("{:?}", Const::Bytes(crate::testing::ebcdic(text)));
    assert_eq!(fields("WITH FILLER ALL TO VALUE"), [field(0, &number, false), field(1, &bytes("F"), true), field(2, &bytes("AB"), true)]);
    assert_eq!(fields("REPLACING NUMERIC BY N"), [field(0, "N", false)]);
    assert_eq!(fields("ALPHANUMERIC-EDITED TO VALUE THEN TO DEFAULT"), [field(0, "Zero", false), field(2, &bytes("AB"), true)]);
}

#[test]
fn set_to_entry_names_its_entry_and_a_call_through_any_other_pointer_finds_it() {
    let data = "       01  PP USAGE PROCEDURE-POINTER.\n       01  PGM PIC X(8) VALUE 'SUB'.\n       01  A PIC X.\n";
    let p = lowered(&program("", data, &[line("SET PP TO ENTRY PGM"), line("CALL PP USING A"), line("GOBACK.")].concat()));
    let Op::SetEntry { entry: LirOperand::Load(pgm), targets } = &p.blocks[0].ops[0] else { panic!("{:?}", p.blocks[0].ops) };
    assert_eq!((symbol(&p, p.places[*pgm as usize].name), targets.len()), ("PGM", 1));
    let call = &p.services.calls[0];
    let CallTarget::Entry(pointer) = call.target else { panic!("{call:?}") };
    assert_eq!(symbol(&p, p.places[pointer as usize].name), "PP");
    assert!(matches!(call.args[..], [CallArg::Reference(_)]), "{call:?}");
}

#[test]
fn constructs_outside_the_slice_are_refused_by_name() {
    let refused = |body: &str, data: &str| lower(&compiled(&program("", data, &[line(body), line("GOBACK.")].concat()))).unwrap_err();
    let mixed = refused("MOVE FUNCTION MAX(A 1) TO A", "       01  A PIC X.\n");
    assert!(matches!(mixed, LowerError::Unsupported("FUNCTION MIN or MAX of arguments of different kinds", _)));
    let odo = "       01  A PIC X.\n       01  C PIC 9.\n       01  G.\n           05 T PIC 9 OCCURS 1 TO 3 DEPENDING ON C.\n";
    let all = refused("COMPUTE C = FUNCTION SQRT(T(ALL))", odo);
    assert!(matches!(all, LowerError::Unsupported("a FUNCTION of fixed arguments given a table whose ALL subscripts run to an OCCURS DEPENDING ON count", _)));
    let numval = refused("MOVE FUNCTION MAX(N M) TO A", "       01  A PIC X.\n       01  N PIC 9.\n       01  M PIC 99.\n");
    assert!(matches!(numval, LowerError::Unsupported(n, _) if n.starts_with("a FUNCTION result whose digits")));
}

#[test]
fn declaratives_that_never_run_lower_as_paragraphs_before_the_procedure_start() {
    let data = "       01  K PIC 9 VALUE 1.\n       01  J PIC 9.\n       01  T.\n           05 E PIC X OCCURS 3.\n";
    // A line indented four columns is in area B; any other starts in area A.
    let source = |lines: &[&str]| lines.iter().map(|l| if l.starts_with("    ") { line(l.trim_start()) } else { format!("       {l}\n") }).collect::<String>();
    let inert = ["DECLARATIVES.", "S SECTION.", "    USE AFTER STANDARD ERROR PROCEDURE ON INPUT.", "P.", "    CONTINUE.", "END DECLARATIVES.", "A.", "    GOBACK."];
    let p = lowered(&program("", data, &source(&inert)));
    assert_eq!(p.procedure_start, 2);
}

#[test]
fn lowering_is_deterministic() {
    let source = program(
        "",
        "       01  N PIC 99 VALUE 15.\n          88 TEENS VALUE 13 THRU 19.\n       01  T.\n           05 E PIC X OCCURS 3.\n",
        &[line("EVALUATE TRUE WHEN TEENS DISPLAY 'T' WHEN OTHER INITIALIZE T END-EVALUATE"), line("GOBACK.")].concat(),
    );
    let c = compiled(&source);
    assert_eq!(encoded(&lower(&c).unwrap()), encoded(&lower(&c).unwrap()));
}

#[test]
fn local_storage_and_linkage_places_have_their_own_bases() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  W PIC X(3) VALUE 'WWW'.\n       LOCAL-STORAGE SECTION.\n       01  L PIC 9(3) VALUE 7.\n",
        "       LINKAGE SECTION.\n       01  LK PIC X(3).\n       PROCEDURE DIVISION USING LK.\n",
        &line("MOVE W TO LK"),
        &line("MOVE L TO W"),
        &line("GOBACK."),
    ]
    .concat();
    let p = lowered(&source);
    assert_eq!(place_named(&p, "W")[0].base, Base::Program);
    assert_eq!(place_named(&p, "L")[0].base, Base::Local);
    assert_eq!(place_named(&p, "LK")[0].base, Base::Linkage(0));
    assert_eq!((p.storage.linkage.as_slice(), p.storage.using.as_slice()), (&[3][..], &[0][..]));
    assert_eq!(p.storage.local_image, [0xF0, 0xF0, 0xF7]);
    assert_eq!(&p.storage.image[..3], &[0xE6; 3]);
}

const TABLE: &str = "       01  T.\n           05 V PIC 9(3) OCCURS 3.\n       01  J PIC 9 VALUE 4.\n       01  A PIC 9(3)V9 VALUE 1.\n       01  X COMP-2.\n";

#[test]
fn a_subscript_or_exponent_expression_locates_its_operands_before_it_evaluates_them() {
    let p = lowered(&program("SSRANGE", TABLE, &[line("MOVE V(A + V(J)) TO A"), line("COMPUTE A = A ** (V(J) - A)"), line("GOBACK.")].concat()));
    let fixed: Vec<_> = p.places.iter().flat_map(|q| &q.subscripts).map(|s| &s.value).filter(|v| matches!(v, IntExpr::Fixed { .. })).collect();
    let IntExpr::Fixed { dmax, prepass, .. } = fixed[0] else { unreachable!() };
    assert_eq!(*dmax, 1);
    assert_eq!(prepass.len(), 1, "A is static; V(J) is located before A is read");
    assert_eq!(p.symbols[p.places[prepass[0] as usize].name as usize], "V");
    let pow = p.exprs.iter().find_map(|e| if let lir::Expr::Pow(_, n) = e { Some(n.clone()) } else { None }).unwrap();
    assert!(matches!(pow, IntExpr::Fixed { dmax: 1, ref prepass, .. } if prepass.len() == 1), "{pow:?}");
}

#[test]
fn a_compared_expression_keeps_its_float_test_its_dmax_pass_and_its_mode() {
    let p = lowered(&program("SSRANGE", TABLE, &[line("IF A + V(J) > 5 DISPLAY 'F' END-IF"), line("IF X * V(J) < 1 DISPLAY 'X' END-IF"), line("GOBACK.")].concat()));
    let exprs: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Rel { a, how, .. } = c { Some((a.clone(), *how)) } else { None }).collect();
    let (Comparand::Expr { dmax, mode, prepass, .. }, how) = &exprs[0] else { panic!("{:?}", exprs[0]) };
    assert_eq!((*dmax, *mode, *how), (1, Mode::Fixed, lir::Compare::Fixed));
    assert_eq!(prepass.len(), 2, "V(J) by the float test, then again by the dmax pass");
    assert_eq!(prepass[0], prepass[1]);
    let (Comparand::Expr { dmax, mode, prepass, .. }, how) = &exprs[1] else { panic!("{:?}", exprs[1]) };
    assert_eq!((*dmax, *how), (0, lir::Compare::Float));
    assert!(matches!(mode, Mode::Float(_)));
    assert!(prepass.is_empty(), "the float test stops at X, which is static, and float mode has no dmax pass");
}

#[test]
fn a_sign_condition_reads_an_operand_and_evaluates_an_expression() {
    let p = lowered(&program("SSRANGE", TABLE, &[line("IF A IS POSITIVE DISPLAY 'P' END-IF"), line("IF A - V(J) IS NEGATIVE DISPLAY 'N' END-IF"), line("GOBACK.")].concat()));
    let signs: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Sign { value, test } = c { Some((value.clone(), *test)) } else { None }).collect();
    assert!(matches!(signs[0], (Comparand::Operand(LirOperand::Load(_)), SignTest::Positive)));
    assert!(matches!(&signs[1], (Comparand::Expr { mode: Mode::Fixed, dmax: 1, prepass, .. }, SignTest::Negative) if prepass.len() == 2));
}

#[test]
fn perform_varying_by_a_subscripted_item_locates_it_before_the_add() {
    let p = lowered(&program("SSRANGE", TABLE, &[line("PERFORM VARYING A FROM 1 BY V(J) UNTIL A > 9"), line("    CONTINUE"), line("END-PERFORM"), line("GOBACK.")].concat()));
    let (plan, prepass) = ops(&p).find_map(|op| if let Op::Step { plan, prepass, .. } = op { Some((*plan, prepass.clone())) } else { None }).unwrap();
    assert_eq!(plan.dmax, 1);
    assert_eq!(prepass.len(), 1);
    assert_eq!(p.symbols[p.places[prepass[0] as usize].name as usize], "V");
}

#[test]
fn perform_varying_a_subscripted_variable_locates_it_again_for_the_step_s_dmax_pass() {
    let p = lowered(&program("SSRANGE", TABLE, &[line("PERFORM VARYING V(J) FROM 1 BY 1 UNTIL A > 9"), line("    CONTINUE"), line("END-PERFORM"), line("GOBACK.")].concat()));
    let (var, prepass) = ops(&p).find_map(|op| if let Op::Step { var, prepass, .. } = op { Some((*var, prepass.clone())) } else { None }).unwrap();
    assert_eq!(prepass, [var]);
}

#[test]
fn a_value_clause_s_abend_names_its_data_entry() {
    let p = lowered(&program("", "       01  A PIC X(3) VALUE 'AB'.\n       01  N PIC X VALUE 'ą'.\n", &line("GOBACK.")));
    let abend = &p.abends[p.storage.init_abend.unwrap() as usize];
    let at = p.debug.positions[abend.at.unwrap() as usize];
    assert_eq!((at.line, at.col), (6, 8));
    assert_eq!(&p.storage.image[..3], &[0xC1, 0xC2, 0x40]);
}

#[test]
fn the_program_collating_sequence_lowers_as_the_program_s_own() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS PCS.\n",
        "       SPECIAL-NAMES.\n           ALPHABET PCS IS 'Z' THROUGH 'A' '0' ALSO '9', HIGH-VALUE.\n",
        "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  C PIC X VALUE HIGH-VALUE.\n",
        "       PROCEDURE DIVISION.\n           IF C > 'A' MOVE LOW-VALUE TO C END-IF\n           GOBACK.\n",
    );
    let p = lowered(source);
    let Collating::Sequence(s) = &p.options.collating else { panic!("{:?}", p.options.collating) };
    let c = compiled(source);
    assert_eq!(*s.positions, c.collating.positions());
    assert_eq!((s.high_value, s.low_value), (c.collating.high_value, c.collating.low_value));
    assert_eq!((s.characters.len(), s.characters[0], s.low_value), (c.collating.count(), 0xE9, 0xE9));
    assert_eq!(s.positions[0xF0], s.positions[0xF9]);
    assert_eq!(p.storage.image, [c.collating.high_value]);
    let native = lowered(&program("", "", &line("GOBACK.")));
    assert_eq!(native.options.collating, Collating::Native);
}

/// Each program of bench/*.cbl, through the check the Harness makes of every test program.
#[test]
fn bench_programs_lower_or_name_what_they_lack() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench");
    let mut benches: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "cbl")).collect();
    benches.sort();
    assert!(!benches.is_empty());
    for path in &benches {
        let text = syntax::copy::decode(&std::fs::read(path).unwrap());
        let origin = format!("bench/{}", path.file_name().unwrap().to_string_lossy());
        for program in syntax::parse_all_with(&text, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{origin}: {e}")) {
            let c = crate::compile(program, &[]).unwrap_or_else(|e| panic!("{origin}: {e:?}"));
            check_lowering(&c, rt::sql::fingerprint(&text), Some(&origin));
        }
    }
}

#[test]
fn a_condition_name_whose_values_compare_differently_is_an_or_of_relations_in_value_order() {
    let p = lowered(&program("", "       01  N PIC 9.\n          88 NONE VALUE ZERO SPACE.\n", &[line("IF NONE DISPLAY 'NONE' END-IF"), line("GOBACK.")].concat()));
    assert!(!p.conds.iter().any(|c| matches!(c, LirCond::Name { .. })));
    let hows: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Rel { op: ast::RelOp::Eq, how, .. } = c { Some(*how) } else { None }).collect();
    assert_eq!(hows, [lir::Compare::Fixed, lir::Compare::ZonedBytes { zoned_first: true }]);
    let or = p.conds.iter().find_map(|c| if let LirCond::Or(a, b) = c { Some((*a, *b)) } else { None }).unwrap();
    assert_eq!(or, (0, 1));
}

#[test]
fn under_invdata_an_unsigned_zoned_integer_compares_with_zero_or_its_like_by_bytes() {
    let data = "       01  V PIC 9(4).\n       01  W PIC 9(4).\n       01  S PIC S9(4).\n       01  L PIC 9(5).\n";
    let procedure = [line("IF V = ZERO OR V = W OR S = ZERO OR V = L DISPLAY 'Y' END-IF"), line("GOBACK.")].concat();
    let hows = |options: &str| {
        let p = lower(&compiled(&program(options, data, &procedure))).unwrap_or_else(|e| panic!("{e}"));
        p.conds.iter().filter_map(|c| if let LirCond::Rel { how, .. } = c { Some(*how) } else { None }).collect::<Vec<_>>()
    };
    let bytes = lir::Compare::ZonedBytes { zoned_first: true };
    assert_eq!(hows("INVDATA"), [bytes, bytes, lir::Compare::Fixed, lir::Compare::Fixed]);
    assert_eq!(hows("OPT(2)"), [bytes, bytes, lir::Compare::Fixed, lir::Compare::Fixed]);
    assert_eq!(hows("INVDATA(FNC)"), [lir::Compare::Fixed; 4]);
    assert_eq!(hows("INVDATA(FNC),OPT(2)"), [lir::Compare::Fixed; 4]);
    assert_eq!(hows(""), [lir::Compare::Fixed; 4]);
}

#[test]
fn optimized_a_condition_name_of_value_zero_compares_its_unsigned_zoned_variable_by_bytes() {
    let data = "       01  F PIC 9.
          88 ENTERED VALUE 0.
          88 STARTED VALUE 0 5.
";
    let how = |options: &str, name: &str| {
        let p = lowered(&program(options, data, &[line(&format!("IF {name} DISPLAY 'Y' END-IF")), line("GOBACK.")].concat()));
        let names: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Name { how, .. } = c { Some(*how) } else { None }).collect();
        let rels: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Rel { how, .. } = c { Some(*how) } else { None }).collect();
        (names, rels)
    };
    let bytes = lir::Compare::ZonedBytes { zoned_first: true };
    assert_eq!(how("", "ENTERED"), (vec![lir::Compare::Fixed], vec![]));
    assert_eq!(how("OPT(1)", "ENTERED"), (vec![bytes], vec![]));
    assert_eq!(how("OPT(2)", "STARTED"), (vec![], vec![bytes, lir::Compare::Fixed]));
}

/// Where control goes from a block through plain jumps to blocks with no ops.
fn through(p: &Program, mut b: u32) -> u32 {
    while let (true, Terminator::Jump(t)) = (p.blocks[b as usize].ops.is_empty(), &p.blocks[b as usize].end) {
        b = *t;
    }
    b
}

fn symbol(p: &Program, id: u32) -> &str {
    &p.symbols[id as usize]
}

#[test]
fn go_to_depending_on_switches_and_alter_sets_the_go_to_its_paragraph_s_entry_takes() {
    let p = lowered(&program(
        "",
        "       01  K PIC 9 VALUE 2.\n",
        &[
            "       MAIN-LINE.\n",
            &line("GO TO P1 P2 DEPENDING ON K"),
            &line("ALTER SW TO PROCEED TO P2"),
            &line("GO TO SW."),
            "       SW.\n",
            &line("GO TO P1."),
            "       P1.\n",
            &line("DISPLAY 'P1'."),
            "       P2.\n",
            &line("GO TO."),
        ]
        .concat(),
    ));
    let (sw, p1, p2) = (paragraph(&p, "SW") as u32, paragraph(&p, "P1") as u32, paragraph(&p, "P2") as u32);
    let switch = p.blocks.iter().find_map(|b| if let Terminator::Switch { value, targets, otherwise } = &b.end { Some((value.clone(), targets.clone(), *otherwise)) } else { None });
    let (value, targets, otherwise) = switch.unwrap();
    assert!(matches!(value, IntExpr::Item(k) if symbol(&p, p.places[k as usize].name) == "K"));
    assert_eq!(targets, [p1, p2]);
    assert_eq!(p.blocks[otherwise as usize].ops, [Op::Alter { para: sw, to: p2 }]);
    let entry = &p.blocks[p.paragraphs[sw as usize].entry as usize];
    let Terminator::AlteredGoTo { para, otherwise } = entry.end else { panic!("{entry:?}") };
    assert_eq!((para, entry.ops.len()), (sw, 0));
    assert_eq!(p.blocks[otherwise as usize].end, Terminator::Jump(p.paragraphs[p1 as usize].entry));
    assert_eq!(end_of(&p, "P2"), Terminator::ParagraphEnd { next: p2 + 1 });
    assert!(!ops(&p).any(|op| matches!(op, Op::EnterSegment(_))), "no ALTER names a paragraph of an independent segment");
    assert!(p.paragraphs.iter().all(|q| q.priority == 0));
}

#[test]
fn an_altered_paragraph_of_an_independent_segment_makes_every_entry_enter_its_segment() {
    let segment = |name: &str, priority: u8| {
        [
            format!("       {name} SECTION {priority}.\n       {name}-START.\n"),
            line(&format!("DISPLAY 'IN {priority}'.")),
            format!("       {name}-SW.\n"),
            line(&format!("GO TO {name}-FIRST.")),
            format!("       {name}-FIRST.\n"),
            line(&format!("ALTER {name}-SW TO PROCEED TO {name}-SECOND")),
            line(&format!("GO TO {name}-SW.")),
            format!("       {name}-SECOND.\n"),
            line("DISPLAY 'SECOND'."),
        ]
        .concat()
    };
    let p = lowered(&program(
        "",
        "",
        &["       MAIN SECTION.\n".to_owned(), line("PERFORM FIXED"), line("PERFORM INDEP 2 TIMES"), line("GOBACK."), segment("FIXED", 10), segment("INDEP", 50)].concat(),
    ));
    for q in &p.paragraphs {
        assert_eq!(p.blocks[q.entry as usize].ops.first(), Some(&Op::EnterSegment(q.priority)), "{}", symbol(&p, q.name));
    }
    assert_eq!(p.paragraphs[paragraph(&p, "INDEP-SW")].priority, 50);
    for sw in ["FIXED-SW", "INDEP-SW"] {
        assert!(matches!(p.blocks[p.paragraphs[paragraph(&p, sw)].entry as usize].end, Terminator::AlteredGoTo { .. }), "{sw}");
    }
    // A completing frame puts the segment register back, and a resume sets it to its paragraph's.
    let enters: Vec<(u32, Option<lir::Resume>)> = p.blocks.iter().filter_map(|b| if let Terminator::PerformEnter { ret, resume, .. } = b.end { Some((ret, resume)) } else { None }).collect();
    assert_eq!(enters.len(), 2);
    assert_eq!(enters.iter().map(|(ret, _)| p.blocks[*ret as usize].ops.clone()).collect::<Vec<_>>(), [vec![Op::Unnest(1)], vec![]]);
    assert_eq!((enters[0].1.map(|r| r.para), enters[1].1), (Some(paragraph(&p, "MAIN") as u32), None));
}

#[test]
fn an_entry_statement_starts_a_block_a_call_of_its_name_enters() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUBPROG.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  COUNTER PIC 9 VALUE 0.\n       LINKAGE SECTION.\n       01  PAYREC PIC X(5).\n       01  PAY-CODE PIC 9.\n",
        "       PROCEDURE DIVISION USING PAYREC.\n",
        &line("ADD 1 TO COUNTER"),
        &line("ENTRY 'PASSED'."),
        &line("EXIT PROGRAM."),
        &line("ENTRY 'PAYMASTR' USING PAY-CODE PAYREC."),
        &line("ADD 2 TO COUNTER"),
        &line("GOBACK."),
    ]
    .concat();
    let p = lowered(&source);
    let entries = &p.services.entries;
    assert_eq!(entries.iter().map(|e| symbol(&p, e.name)).collect::<Vec<_>>(), ["PASSED", "PAYMASTR"]);
    assert_eq!((entries[0].using.as_slice(), entries[1].using.as_slice()), (&[][..], &[1, 0][..]));
    assert!(entries.iter().all(|e| e.paragraph == 0));
    assert!(matches!(p.blocks[through(&p, entries[0].block) as usize].end, Terminator::ExitProgram { .. }));
    let paymastr = &p.blocks[through(&p, entries[1].block) as usize];
    assert!(matches!(paymastr.ops.first(), Some(Op::Arith(1))), "{paymastr:?}");
}

#[test]
fn perform_varying_after_steps_the_outer_variable_before_it_sets_the_inner_one_again() {
    let data = "       01  I PIC 9(2).\n       01  J PIC 9(2).\n";
    let body = |test: &str| {
        [line(&format!("PERFORM P {test}VARYING I FROM 1 BY 1 UNTIL I > 3")), line("    AFTER J FROM I BY 1 UNTIL J > 3"), line("GOBACK."), "       P.\n".into(), line("DISPLAY I J.")].concat()
    };
    let var = |p: &Program, op: &Op| match op {
        Op::Move { to, .. } | Op::Set { to, .. } => format!("MOVE {}", symbol(p, p.places[*to as usize].name)),
        Op::Step { var, .. } => format!("STEP {}", symbol(p, p.places[*var as usize].name)),
        other => format!("{other:?}"),
    };
    let blocks = |p: &Program| p.blocks.iter().map(|b| b.ops.iter().map(|op| var(p, op)).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let before = lowered(&program("", data, &body("")));
    let shown = blocks(&before);
    assert_eq!(shown[0], "Nest, MOVE I, MOVE J");
    assert!(shown.contains(&"STEP I, MOVE J".to_owned()) && shown.contains(&"STEP J".to_owned()), "{shown:?}");
    let after = lowered(&program("", data, &body("WITH TEST AFTER ")));
    let shown = blocks(&after);
    assert_eq!(shown[0], "Nest, MOVE I");
    assert!(shown.contains(&"MOVE J".to_owned()) && shown.contains(&"STEP I".to_owned()) && shown.contains(&"STEP J".to_owned()), "{shown:?}");
}

#[test]
fn call_plans_keep_each_argument_as_the_walker_passes_it() {
    let data = concat!(
        "       01  PGM PIC X(8) VALUE 'SUB'.\n       01  REC PIC X(5).\n       01  N PIC S9(4) COMP.\n",
        "       01  JNINATIVEINTERFACE.\n           02  FP USAGE FUNCTION-POINTER.\n",
    );
    let body = [
        "CALL 'CEEDATE' USING REC",
        "CALL 'sub1 ' USING BY REFERENCE REC",
        "    BY CONTENT 'AB' 12 ZERO LENGTH OF REC BY VALUE N OMITTED",
        "    RETURNING N ON EXCEPTION DISPLAY 'MISSING'",
        "    NOT ON EXCEPTION DISPLAY 'CALLED' END-CALL",
        "CALL PGM",
        "CALL FP USING REC OMITTED",
        "CANCEL PGM 'SUB'",
        "GOBACK.",
    ]
    .map(line)
    .concat();
    let p = lowered(&program("", data, &body));
    let calls = &p.services.calls;
    let CallTarget::Named { name, le } = calls[0].target else { panic!("{:?}", calls[0].target) };
    assert_eq!((symbol(&p, name), le), ("CEEDATE", Some(LeService::Ceedate)));
    let CallTarget::Named { name, le: None } = calls[1].target else { panic!("{:?}", calls[1].target) };
    assert_eq!(symbol(&p, name), "SUB1");
    assert!(matches!(calls[0].args[0], CallArg::Reference(_)) && matches!(calls[1].args[0], CallArg::Reference(_)));
    assert_eq!(calls[1].args[1], CallArg::Content(Chars::Literal(vec![0xC1, 0xC2])));
    assert_eq!(calls[1].args[2], CallArg::Content(Chars::Literal(vec![0xF1, 0xF2])));
    assert_eq!(calls[1].args[3], CallArg::Content(Chars::Literal(vec![0xF0])));
    assert!(matches!(calls[1].args[4], CallArg::Content(Chars::Value(LirOperand::LengthOf(_)))));
    assert!(matches!(calls[1].args[5], CallArg::Value(LirOperand::Load(_))));
    assert_eq!(calls[1].args[6], CallArg::Omitted);
    assert!(calls[1].returning.is_some() && calls[1].on_exception && calls[1].not_on_exception);
    assert!(matches!(calls[2].target, CallTarget::Dynamic(LirOperand::Load(_))));
    assert!(matches!(calls[3].target, CallTarget::Pointer(_)));
    assert!(matches!(calls[3].args.as_slice(), [CallArg::Value(LirOperand::Load(_)), CallArg::Omitted]));
    let selects: Vec<_> = p.blocks.iter().filter(|b| matches!(b.end, Terminator::Select(_))).map(|b| b.ops.last().cloned()).collect();
    assert_eq!(selects, [Some(Op::Call(1))]);
    let cancels: Vec<_> = ops(&p).filter_map(|op| if let Op::Cancel(o) = op { Some(*o) } else { None }).collect();
    assert!(matches!(cancels.as_slice(), [LirOperand::Load(_), LirOperand::Const(_)]));
}

const OO_CARD: &str = "       CBL THREAD,DLL\n";

#[test]
fn a_class_definition_lowers_its_data_and_each_method_as_a_program() {
    let method = |name: &str, linkage: &str, header: &str, body: &[&str]| {
        let data = if linkage.is_empty() { String::new() } else { format!("       DATA DIVISION.\n       LINKAGE SECTION.\n{linkage}") };
        let body: String = body.iter().map(|l| line(l)).collect();
        format!("       IDENTIFICATION DIVISION.\n       METHOD-ID. \"{name}\".\n{data}       PROCEDURE DIVISION{header}.\n{body}       END METHOD \"{name}\".\n")
    };
    let part = |kind: &str, data: &str, methods: &[String]| {
        format!("       IDENTIFICATION DIVISION.\n       {kind}.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{}       END {kind}.\n", methods.concat())
    };
    let source = [
        OO_CARD,
        "       IDENTIFICATION DIVISION.\n       CLASS-ID. Account INHERITS Base.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n",
        "           CLASS Base IS \"java.lang.Object\"\n           CLASS Account IS \"Account\".\n",
        &part(
            "FACTORY",
            "       01  OPENED PIC S9(9) BINARY VALUE 0.\n",
            &[method("open", "       01  OPENING USAGE OBJECT REFERENCE Account.\n", " RETURNING OPENING", &["INVOKE Account NEW RETURNING OPENING", "ADD 1 TO OPENED."])],
        ),
        &part(
            "OBJECT",
            "       01  BALANCE PIC S9(9) BINARY VALUE 100.\n",
            &[
                method("credit", "       01  AMOUNT PIC S9(9) BINARY.\n", " USING BY VALUE AMOUNT", &["ADD AMOUNT TO BALANCE", "INVOKE SELF \"show\"."]),
                method("show", "", "", &["DISPLAY BALANCE."]),
            ],
        ),
        "       END CLASS Account.\n",
    ]
    .concat();
    let p = lowered(&source);
    let class = p.services.class.as_deref().unwrap();
    assert_eq!((symbol(&p, class.external), symbol(&p, class.parent)), ("Account", "java.lang.Object"));
    let (factory, object) = (class.factory.as_ref().unwrap(), class.object.as_ref().unwrap());
    assert_eq!((factory.records.as_slice(), factory.data.storage.image.as_slice()), (&[0][..], &[0, 0, 0, 0][..]));
    assert_eq!((object.records.as_slice(), object.data.storage.image.as_slice()), (&[0][..], &[0, 0, 0, 100][..]));
    let methods: Vec<_> = class.methods.iter().map(|m| (symbol(&p, m.name), m.factory, m.params.iter().map(|&s| symbol(&p, s)).collect::<Vec<_>>(), m.returns.map(|s| symbol(&p, s)), m.own_records)).collect();
    assert_eq!(
        methods,
        [("open", true, vec![], Some("LAccount;"), 1), ("credit", false, vec!["I"], None, 1), ("show", false, vec![], None, 0)]
    );
    let open = &class.methods[0].code;
    let new = &open.services.invokes[0];
    let Receiver::Class { name, external } = new.receiver else { panic!("{:?}", new.receiver) };
    assert_eq!((symbol(open, name), symbol(open, external), new.method), ("ACCOUNT", "Account", MethodName::New));
    assert_eq!(new.returning.map(|(_, java)| symbol(open, java)), Some("LAccount;"));
    assert_eq!(place_named(open, "OPENED")[0].base, Base::Linkage(1));
    let credit = &class.methods[1].code;
    let show = &credit.services.invokes[0];
    assert_eq!(show.receiver, Receiver::SelfRef);
    assert!(matches!(show.method, MethodName::Named(s) if symbol(credit, s) == "show"));
    assert_eq!(place_named(credit, "BALANCE")[0].base, Base::Linkage(1));
    assert!(credit.services.class.is_none());
}

#[test]
fn invoke_plans_name_the_receiver_method_and_java_types_and_object_references_compare_as_objects() {
    let source = [
        OO_CARD,
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CLIENT RECURSIVE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n",
        "           CLASS Account IS \"Account\".\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  A1 USAGE OBJECT REFERENCE Account.\n       01  A2 USAGE OBJECT REFERENCE Account.\n       01  U USAGE OBJECT REFERENCE.\n",
        "       01  AMOUNT PIC S9(9) BINARY.\n       01  MNAME PIC X(20).\n       PROCEDURE DIVISION.\n",
        &line("INVOKE Account \"open\" RETURNING A1"),
        &line("INVOKE A1 \"credit\" USING BY VALUE AMOUNT 7"),
        &line("    ON EXCEPTION DISPLAY 'NONE' END-INVOKE"),
        &line("INVOKE U MNAME"),
        &line("IF A1 = A2 DISPLAY 'SAME' END-IF"),
        &line("IF A1 = NULL DISPLAY 'NULL' END-IF"),
        &line("GOBACK."),
    ]
    .concat();
    let p = lowered(&source);
    let invokes = &p.services.invokes;
    assert!(matches!(invokes[0].receiver, Receiver::Class { .. }));
    assert_eq!(invokes[0].returning.map(|(_, java)| symbol(&p, java)), Some("LAccount;"));
    assert!(matches!(invokes[1].receiver, Receiver::Object(a) if symbol(&p, p.places[a as usize].name) == "A1"));
    let args: Vec<_> = invokes[1].args.iter().map(|(o, java)| (matches!(o, LirOperand::Load(_)), symbol(&p, *java))).collect();
    assert_eq!(args, [(true, "I"), (false, "I")]);
    assert!(invokes[1].on_exception && !invokes[1].not_on_exception && !invokes[0].on_exception);
    assert!(matches!(invokes[2].method, MethodName::Dynamic(_)));
    let selects: Vec<_> = p.blocks.iter().filter(|b| matches!(b.end, Terminator::Select(_))).map(|b| b.ops.last().cloned()).collect();
    assert_eq!(selects, [Some(Op::Invoke(1))]);
    let hows: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::Rel { how, .. } = c { Some(*how) } else { None }).collect();
    assert_eq!(hows, [lir::Compare::References, lir::Compare::Address]);
}

/// A program with a FILE SECTION: `select` and `fd` lines as written, a line indented four columns
/// in area B.
fn with_files(options: &str, select: &[&str], fd: &[&str], data: &str, procedure: &[&str]) -> String {
    let area = |lines: &[&str]| lines.iter().map(|l| if l.starts_with("    ") { line(l.trim_start()) } else { format!("       {l}\n") }).collect::<String>();
    let card = if options.is_empty() { String::new() } else { format!("       CBL {options}\n") };
    [
        card,
        area(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. T.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL."]),
        area(select),
        area(&["DATA DIVISION.", "FILE SECTION."]),
        area(fd),
        area(&["WORKING-STORAGE SECTION."]),
        data.to_owned(),
        area(&["PROCEDURE DIVISION."]),
        area(procedure),
    ]
    .concat()
}

fn file_ops(p: &Program) -> Vec<(lir::FileOp, usize)> {
    p.blocks
        .iter()
        .flat_map(|b| b.ops.iter().enumerate().map(move |(k, op)| (b, k, op)))
        .filter_map(|(b, k, op)| match op {
            Op::File(id) => Some((p.services.file_ops[*id as usize].clone(), if k + 1 == b.ops.len() { if let Terminator::Select(arms) = &b.end { arms.len() } else { 0 } } else { 0 })),
            _ => None,
        })
        .collect()
}

#[test]
fn a_sequential_read_takes_at_end_and_file_status_takes_the_status_by_move() {
    let p = lowered(&with_files(
        "",
        &["    SELECT IN-F ASSIGN TO INDD FILE STATUS IS FS."],
        &["FD  IN-F.", "01  IN-REC PIC X(8)."],
        "       01  FS PIC XX.\n       01  W PIC X(10).\n",
        &[
            "M.",
            "    OPEN INPUT IN-F",
            "    READ IN-F INTO W",
            "        AT END DISPLAY 'E'",
            "        NOT AT END DISPLAY 'N'",
            "    END-READ",
            "    READ IN-F",
            "    CLOSE IN-F",
            "    GOBACK.",
        ],
    ));
    let file = &p.services.files[0];
    assert_eq!((symbol(&p, file.name), symbol(&p, file.assign)), ("IN-F", "INDD"));
    assert_eq!((file.organization, file.access, file.format, file.keys.is_none()), (lir::Organization::Sequential, lir::Access::Sequential, rt::files::Format::Fixed, true));
    let (status, plan) = file.status.unwrap();
    assert_eq!((symbol(&p, p.places[status as usize].name), plan), ("FS", MovePlan::Alnum { image: Image::Bytes, justified: false }));
    let ops = file_ops(&p);
    let verbs: Vec<_> = ops.iter().map(|(op, arms)| (std::mem::discriminant(&op.verb), op.phrase, *arms)).collect();
    let phrase = lir::Phrase { on: true, not_on: true };
    assert_eq!(verbs.iter().map(|v| (v.1, v.2)).collect::<Vec<_>>(), [(None, 0), (Some(phrase), 3), (None, 0), (None, 0)]);
    let lir::FileVerb::Read { sequential: true, previous: false, into: Some((into, into_plan)), key: 0 } = ops[1].0.verb else { panic!("{:?}", ops[1].0.verb) };
    assert_eq!((symbol(&p, p.places[into as usize].name), into_plan), ("W", MovePlan::Alnum { image: Image::Bytes, justified: false }));
    let select = p.blocks.iter().find_map(|b| if let Terminator::Select(arms) = &b.end { Some(arms.clone()) } else { None }).unwrap();
    assert!(select[1] != select[0] && select[2] != select[0] && select[1] != select[2]);
    assert_eq!(p.blocks[select[1] as usize].ops.len(), 1);
    verify(&p).unwrap();
}

#[test]
fn an_indexed_file_s_keys_are_spans_of_its_record_and_a_keyed_read_or_start_names_one() {
    let p = lowered(&with_files(
        "",
        &["    SELECT MF ASSIGN TO MDD", "        ORGANIZATION IS INDEXED ACCESS MODE IS DYNAMIC", "        RECORD KEY IS MK", "        ALTERNATE RECORD KEY IS AK WITH DUPLICATES."],
        &["FD  MF.", "01  MR.", "    05 MK PIC X(4).", "    05 AK PIC X(6).", "    05 MD PIC X(10)."],
        "       01  PART PIC X(2).\n",
        &[
            "M.",
            "    OPEN I-O MF",
            "    READ MF KEY IS AK",
            "        INVALID KEY DISPLAY 'NO'",
            "    END-READ",
            "    READ MF NEXT AT END DISPLAY 'END' END-READ",
            "    START MF KEY IS NOT LESS THAN MK",
            "        INVALID KEY DISPLAY 'NO'",
            "    END-START",
            "    DELETE MF",
            "    GOBACK.",
        ],
    ));
    let keys = p.services.files[0].keys.clone().unwrap();
    assert_eq!(keys.prime, lir::RecordSpan { offset: 0, len: 4 });
    assert_eq!(keys.alternates, [(lir::RecordSpan { offset: 4, len: 6 }, true)]);
    let ops = file_ops(&p);
    assert!(matches!(ops[1].0.verb, lir::FileVerb::Read { sequential: false, key: 1, .. }));
    assert_eq!((ops[1].0.phrase, ops[1].1), (Some(lir::Phrase { on: true, not_on: false }), 3));
    assert!(matches!(ops[2].0.verb, lir::FileVerb::Read { sequential: true, .. }));
    let start = lir::FileVerb::Start { rel: lir::StartRel::NotLess, key: lir::StartKey::Named { key: 0, span: lir::RecordSpan { offset: 0, len: 4 } } };
    assert_eq!(ops[3].0.verb, start);
    assert_eq!((ops[4].0.verb.clone(), ops[4].1), (lir::FileVerb::Delete, 0));
}

#[test]
fn write_from_advancing_on_a_linage_file_selects_five_arms() {
    let p = lowered(&with_files(
        "",
        &["    SELECT PF ASSIGN TO PDD."],
        &["FD  PF LINAGE IS 10 LINES WITH FOOTING AT 8.", "01  PR PIC X(20)."],
        "       01  W PIC X(20).\n",
        &["M.", "    OPEN OUTPUT PF", "    WRITE PR FROM W AFTER ADVANCING 2 LINES", "        AT END-OF-PAGE DISPLAY 'EOP'", "    END-WRITE", "    GOBACK."],
    ));
    let file = &p.services.files[0];
    let linage = file.linage.clone().unwrap();
    assert_eq!((linage.lines, linage.footing, linage.top), (IntExpr::Const(10), Some(IntExpr::Const(8)), None));
    let (counter, _) = linage.counter.unwrap();
    assert_eq!(symbol(&p, p.places[counter as usize].name), "LINAGE-COUNTER");
    assert_eq!(file.carriage, Some(lir::Carriage { machine: false, reserved: false }));
    let (write, arms) = file_ops(&p)[1].clone();
    assert_eq!((write.phrase, write.end_of_page, arms), (None, Some(lir::Phrase { on: true, not_on: false }), 5));
    let lir::FileVerb::Write { record, from: Some(from), advancing: Some(advancing) } = write.verb else { panic!() };
    assert_eq!((&p.places[record as usize], from.plan), (&p.places[from.to as usize], MovePlan::Alnum { image: Image::Bytes, justified: false }));
    assert_eq!(symbol(&p, p.places[record as usize].name), "PR");
    assert_eq!(advancing, lir::Advance::Lines { before: false, count: IntExpr::Const(2) });
}

#[test]
fn an_exception_procedure_is_a_range_each_file_or_open_mode_names_and_a_perform_it_leaves_can_resume() {
    let source = with_files(
        "",
        &["    SELECT IN-F ASSIGN TO INDD.", "    SELECT OUT-F ASSIGN TO OUTDD."],
        &["FD  IN-F.", "01  IN-REC PIC X(3).", "FD  OUT-F.", "01  OUT-REC PIC X(3)."],
        "       01  K PIC 9 VALUE 0.\n",
        &[
            "DECLARATIVES.",
            "OUT-ERR SECTION.",
            "    USE AFTER EXCEPTION PROCEDURE OUT-F.",
            "E1.",
            "    DISPLAY 'E1'",
            "    IF K = 0 GO TO M2.",
            "E2.",
            "    DISPLAY 'E2'.",
            "IN-ERR SECTION.",
            "    USE AFTER ERROR PROCEDURE ON INPUT.",
            "I1.",
            "    DISPLAY 'I1'.",
            "END DECLARATIVES.",
            "M SECTION.",
            "M1.",
            "    PERFORM E1",
            "    DISPLAY 'BACK'",
            "    GOBACK.",
            "M2.",
            "    MOVE 1 TO K",
            "    OPEN INPUT OUT-F IN-F",
            "    WRITE OUT-REC",
            "    GOBACK.",
        ],
    );
    let p = lowered(&source);
    let range = |first: &str, last: &str, kind| lir::Range { first: paragraph(&p, first) as u32, last: paragraph(&p, last) as u32, kind };
    let out_err = p.services.files[1].error.unwrap();
    assert_eq!((p.services.files[0].error, p.ranges[out_err as usize]), (None, range("OUT-ERR", "E2", lir::RangeKind::UseProcedure)));
    let input = p.services.declaratives.modes[0].unwrap();
    assert_eq!(p.ranges[input as usize], range("IN-ERR", "I1", lir::RangeKind::UseProcedure));
    assert_eq!((&p.services.declaratives.modes[1..], p.services.declaratives.debug_item), (&[None; 3][..], None));
    assert_eq!(file_ops(&p).len(), 3);
    let enter = p.blocks.iter().find_map(|b| if let Terminator::PerformEnter { range, resume, .. } = b.end { Some((range, resume)) } else { None });
    let (performed, resume) = enter.unwrap();
    assert_eq!(p.ranges[performed as usize], range("E1", "E1", lir::RangeKind::Perform));
    assert_eq!(resume.map(|r| r.para), Some(paragraph(&p, "M1") as u32));
    for (name, armed) in [("E1", true), ("E2", true), ("I1", true), ("M1", false), ("OUT-ERR", false)] {
        assert_eq!(p.paragraphs[paragraph(&p, name)].abandoned.is_some(), armed, "{name}");
    }
    assert_eq!(end_of(&p, "E2"), Terminator::ParagraphEnd { next: paragraph(&p, "IN-ERR") as u32 });
    assert!(!p.blocks.iter().any(|b| matches!(b.end, Terminator::Debug { .. })) && !ops(&p).any(|op| matches!(op, Op::DebugLine(_))));
}

/// A program WITH DEBUGGING MODE whose section DBG serves paragraphs SW and P, which the main line
/// reaches by GO TO, an ALTER and fall-through.
fn debugging_program() -> String {
    [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       SOURCE-COMPUTER. IBM-370 WITH DEBUGGING MODE.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n       DBG SECTION.\n",
        "           USE FOR DEBUGGING ON P SW.\n       D1.\n           DISPLAY DEBUG-NAME DEBUG-CONTENTS.\n       END DECLARATIVES.\n",
        "       M SECTION.\n       M1.\n",
        &line("ALTER SW TO PROCEED TO P"),
        &line("GO TO SW."),
        "       SW.\n",
        &line("GO TO Q."),
        "       Q.\n",
        &line("DISPLAY 'Q'."),
        "       P.\n",
        &line("GO TO R."),
        "       R.\n",
        &line("GOBACK."),
    ]
    .concat()
}

#[test]
fn under_the_debug_option_a_debugging_section_runs_at_its_paragraph_s_entry_and_after_an_alter_of_it() {
    let source = debugging_program();
    let off = lowered(&source);
    assert!(!off.blocks.iter().any(|b| matches!(b.end, Terminator::Debug { .. })) && !ops(&off).any(|op| matches!(op, Op::DebugLine(_) | Op::DebugAlter { .. })));
    let p = lower(&compiled_with(&source, &["-debug"])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(round_trip(&p), p);
    let (dbg, d1, pp) = (paragraph(&p, "DBG") as u32, paragraph(&p, "D1") as u32, paragraph(&p, "P") as u32);
    let section = p.ranges.iter().position(|r| *r == lir::Range { first: dbg, last: d1, kind: lir::RangeKind::Debugging }).unwrap() as u32;
    let entry = &p.blocks[p.paragraphs[pp as usize].entry as usize];
    let Terminator::Debug { range, name, next } = entry.end else { panic!("{entry:?}") };
    assert_eq!((range, symbol(&p, name), entry.ops.len()), (section, "P", 0));
    assert!(matches!(p.blocks[next as usize].ops[..], [Op::DebugLine(22), ..]));
    assert_eq!(p.blocks.iter().filter(|b| matches!(b.end, Terminator::Debug { .. })).count(), 2);
    assert_eq!(p.services.declaratives.debug_item.map(|(_, len)| len), Some(86));
    let alter = ops(&p).find_map(|op| if let Op::DebugAlter { range, name, contents } = op { Some((*range, *name, *contents)) } else { None }).unwrap();
    assert_eq!((alter.0, symbol(&p, alter.1), symbol(&p, alter.2)), (section, "SW", "P"));
    // How control came to SW and P is read there, so the GO TO and the fall-through into them stay transfers.
    assert_eq!(end_of(&p, "M1"), Terminator::GoTo(paragraph(&p, "SW") as u32));
    assert_eq!(end_of(&p, "Q"), Terminator::ParagraphEnd { next: pp });
    let m = &p.blocks[p.paragraphs[paragraph(&p, "M")].entry as usize];
    assert_eq!(m.ops, [Op::DebugLine(13)]);
    let mut lines: Vec<u32> = ops(&p).filter_map(|op| if let Op::DebugLine(l) = op { Some(*l) } else { None }).collect();
    lines.sort_unstable();
    assert_eq!(lines, [8, 11, 13, 15, 16, 18, 20, 22, 24], "each section header and statement");
}

#[test]
fn a_group_move_copies_bytes_both_ways_as_the_walker_does() {
    let data = concat!(
        "       01  G.\n           05 G1 PIC X(2).\n           05 G2 PIC X(2).\n       01  Z PIC 9(4).\n       01  P PIC S9(5) COMP-3.\n",
        "       01  E PIC ZZ9.99.\n       01  AE PIC XXBXX.\n       01  F COMP-2.\n       01  PTR POINTER.\n       01  A PIC X(4).\n",
    );
    let body = ["MOVE G TO Z", "MOVE G TO P", "MOVE G TO E", "MOVE G TO AE", "MOVE G TO F", "MOVE Z TO G", "MOVE F TO G", "MOVE 12 TO G", "MOVE G TO A", "SET Z TO G", "GOBACK."].map(line).concat();
    let plans = moves(&lowered(&program("", data, &body)));
    let copied = MovePlan::Alnum { image: Image::Bytes, justified: false };
    let stored = MovePlan::Alnum { image: Image::Stored, justified: false };
    assert_eq!(plans, [copied, copied, copied, copied, copied, stored, stored, MovePlan::Alnum { image: Image::Digits { digits: 2 }, justified: false }, copied, copied]);
    let file = with_files(
        "",
        &["    SELECT OUT-F ASSIGN TO OUTDD."],
        &["FD  OUT-F.", "01  OUT-REC PIC 9(3)."],
        "       01  G.\n           05 G1 PIC X(3).\n",
        &["M.", "    OPEN OUTPUT OUT-F", "    WRITE OUT-REC FROM G", "    GOBACK."],
    );
    let p = lowered(&file);
    let lir::FileVerb::Write { from: Some(from), .. } = file_ops(&p)[1].0.verb else { panic!() };
    assert_eq!(from.plan, copied);
}

#[test]
fn set_lowers_each_form_as_the_walker_runs_it() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  K PIC 9 VALUE 0.\n           88 K-ON VALUE 1 WHEN SET TO FALSE IS 0.\n",
        "       01  J PIC 9 VALUE 0.\n       01  P POINTER.\n       01  W PIC X(4).\n",
        "       01  T.\n           05 E PIC X OCCURS 3 INDEXED BY IX.\n",
        "       LINKAGE SECTION.\n       01  L PIC X(4).\n       PROCEDURE DIVISION.\n",
        &line("SET K-ON TO TRUE"),
        &line("SET K-ON TO FALSE"),
        &line("SET P TO ADDRESS OF W"),
        &line("SET IX TO 2"),
        &line("SET J K UP BY J"),
        &line("SET P IX DOWN BY 1"),
        &line("SET ADDRESS OF L TO P"),
        &line("SET ADDRESS OF L ADDRESS OF W TO NULL"),
        &line("GOBACK."),
    ]
    .concat();
    let p = lowered(&source);
    let name = |q: u32| symbol(&p, p.places[q as usize].name);
    let b = &p.blocks[0].ops;
    let numeric = MovePlan::Numeric { from: NumericFrom::Value, store: StorePlan::Zoned { digits: 1, scale: 0, signed: false, sign: None } };
    let Op::Move { from: LirOperand::Const(on), to, plan, check: SenderCheck::None } = b[0] else { panic!("{:?}", b[0]) };
    assert_eq!((name(to), plan, &p.consts[on as usize]), ("K-ON", numeric, &Const::Number(numeric::precision::Fixed::new(1, numeric::precision::Places::new(1, 0)))));
    let Op::Move { from: LirOperand::Const(off), to, .. } = b[1] else { panic!("{:?}", b[1]) };
    assert!(name(to) == "K-ON" && matches!(p.consts[off as usize], Const::Number(f) if f.magnitude.is_zero()));
    assert!(matches!(b[2], Op::Set { from: LirOperand::AddressOf(_), to, plan: MovePlan::Address } if name(to) == "P"));
    assert!(matches!(b[3], Op::Set { to, plan: MovePlan::Index, .. } if name(to) == "IX"));
    let Op::SetUpDown { by: IntExpr::Item(by), down: false, targets } = &b[4] else { panic!("{:?}", b[4]) };
    assert_eq!((name(*by), targets.iter().map(|t| name(t.0)).collect::<Vec<_>>()), ("J", vec!["J", "K"]));
    let Op::SetUpDown { down: true, targets, .. } = &b[5] else { panic!("{:?}", b[5]) };
    assert!(matches!(targets[..], [(_, lir::UpDown::Pointer), (_, lir::UpDown::Number(_))]));
    assert!(matches!(b[6], Op::SetAddress { ref records, address: LirOperand::Load(q) } if records == &[0] && name(q) == "P"));
    assert!(matches!(&b[7], Op::SetAddress { records, address: LirOperand::Const(_) } if records == &[0]));
    let Terminator::Abend(a) = p.blocks[0].end else { panic!("{:?}", p.blocks[0].end) };
    assert_eq!(symbol(&p, p.abends[a as usize].message), "SET ADDRESS OF W: only a LINKAGE record can be given an address");
}

#[test]
fn a_function_evaluates_its_arguments_then_any_again_as_an_integer_then_its_reference_modification() {
    let p = lowered(&program(
        "",
        "       01  A PIC X(4).\n       01  N PIC 9(3).\n       01  F COMP-2.\n",
        &[
            line("MOVE FUNCTION UPPER-CASE(A)(2:2) TO A"),
            line("MOVE FUNCTION CHAR(N + 1) TO A"),
            line("MOVE FUNCTION MAX(F 1) TO N"),
            line("COMPUTE N = FUNCTION MOD(N 7) + FUNCTION RANDOM"),
            line("DISPLAY FUNCTION TRIM(A LEADING) FUNCTION LENGTH(A N)"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    let f = &p.plans.function;
    assert_eq!((f[0].func, f[0].args.len(), f[0].integer.is_none()), (lir::Func::UpperCase, 1, true));
    assert!(matches!(f[0].refmod, Some(lir::RefMod { start: IntExpr::Const(2), length: Some(IntExpr::Const(2)), check: false })));
    assert!(matches!((&f[1].args[..], &f[1].integer), ([lir::Argument::Value(Comparand::Expr { mode: Mode::Fixed, .. })], Some(IntExpr::Fixed { .. }))));
    assert_eq!(f[2].func, lir::Func::Max);
    assert_eq!(moves(&p)[2], MovePlan::Numeric { from: NumericFrom::Float, store: StorePlan::Zoned { digits: 3, scale: 0, signed: false, sign: None } });
    let compute = &p.plans.arith[0].steps[0];
    assert_eq!(compute.mode, Mode::Float(numeric::Arith::Compat.float_intermediate()));
    assert_eq!((f[5].side, f[5].arity), (Some(lir::TrimSide::Leading), None));
    let arity = f[6].arity.unwrap();
    assert_eq!((f[6].func, symbol(&p, p.abends[arity as usize].message)), (lir::Func::Length, "FUNCTION LENGTH takes 1..=1 arguments"));
    let Op::Display(d) = ops(&p).find(|op| matches!(op, Op::Display(_))).unwrap() else { unreachable!() };
    assert!(matches!(p.plans.display[*d as usize].items[..], [DisplayItem::Value(LirOperand::Function(5)), DisplayItem::Value(LirOperand::Function(6))]));
}

#[test]
fn case_reverse_and_trim_of_a_national_argument_are_national() {
    let statements: String = ["NATIONAL-OF(A)", "REVERSE(W)", "UPPER-CASE(W)", "LOWER-CASE(W)", "TRIM(W)", "REVERSE(A)"].iter().map(|f| line(&format!("MOVE FUNCTION {f} TO W"))).collect();
    let p = lowered(&program("", "       01  A PIC X(4).\n       01  W PIC N(4).\n", &[statements, line("GOBACK.")].concat()));
    let m = moves(&p);
    assert!(m[1..5].iter().all(|plan| *plan == m[0]), "{m:?}");
    assert_ne!(m[5], m[0]);
}

fn with_special_names(clauses: &str, data: &str, procedure: &[String]) -> String {
    program("", data, &procedure.concat()).replace(
        "       DATA DIVISION.\n",
        &format!("       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n{clauses}       DATA DIVISION.\n"),
    )
}

#[test]
fn decimal_point_is_comma_and_currency_signs_reach_the_options_the_edits_and_display() {
    let source = with_special_names(
        "           CURRENCY SIGN IS 'EUR ' WITH PICTURE SYMBOL 'y'\n           DECIMAL-POINT IS COMMA.\n",
        "       01  E PIC yyy9,99.\n       01  Z PIC ZZ9,9.\n",
        &[line("MOVE 1,5 TO E Z"), line("DISPLAY 3,75 ' ' E"), line("GOBACK.")],
    );
    let p = lowered(&source);
    assert!(p.options.decimal_point_comma);
    assert_eq!(p.options.numval_currency, "EUR ");
    let currencies: Vec<&str> = p.edits.iter().map(|e| e.currency.as_str()).collect();
    assert_eq!(currencies, ["EUR ", ""]);
    assert!(p.symbols.iter().any(|s| s == "3,75"));
    let plain = lowered(&program("", "       01  E PIC $$9.99.\n", &[line("DISPLAY 3.75"), line("GOBACK.")].concat()));
    assert_eq!((plain.options.decimal_point_comma, plain.options.numval_currency.as_str(), plain.edits[0].currency.as_str()), (false, "$", "$"));
    assert!(plain.symbols.iter().any(|s| s == "3.75"));
    let two = with_special_names("           CURRENCY 'W'\n           CURRENCY 'CHF' PICTURE SYMBOL 'f'.\n", "       01  E PIC W9.\n", &[line("GOBACK.")]);
    assert_eq!(lowered(&two).options.numval_currency, "$");
}

#[test]
fn a_hexadecimal_currency_sign_reaches_the_edits_and_numval_as_its_code_page_character() {
    let source = with_special_names("           CURRENCY SIGN IS X'9F' WITH PICTURE SYMBOL 'Y'.\n", "       01  E PIC YY9.99.\n", &[line("MOVE 5.5 TO E"), line("GOBACK.")]);
    let p = lowered(&source);
    assert_eq!((p.options.numval_currency.as_str(), p.edits[0].currency.as_str()), ("€", "€"));
    let letter = with_special_names("           CURRENCY SIGN IS X'86'.\n", "       01  E PIC ff9.99.\n", &[line("GOBACK.")]);
    assert_eq!(lowered(&letter).edits[0].currency, "f");
}

#[test]
fn string_unstring_and_inspect_plans_decide_each_receiver_s_store() {
    let p = lowered(&program(
        "",
        "       01  S PIC X(12).\n       01  P PIC 99.\n       01  F1 PIC X(4).\n       01  F2 PIC 9(4).\n       01  D1 PIC X.\n       01  C1 PIC 9.\n       01  T PIC X.\n       01  N PIC 99.\n",
        &[
            line("STRING F1 DELIMITED BY SPACE 12 DELIMITED BY SIZE"),
            line("    INTO S WITH POINTER P"),
            line("    ON OVERFLOW DISPLAY 'OVER' END-STRING"),
            line("UNSTRING S DELIMITED BY ',' OR ALL SPACE"),
            line("    INTO F1 DELIMITER IN D1 COUNT IN C1 F2 TALLYING IN T"),
            line("INSPECT S TALLYING N FOR ALL ZERO"),
            line("    REPLACING ALL 'A' BY SPACE CONVERTING 'ab' TO 'AB'"),
            line("INSPECT S CONVERTING 'ab' TO T"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    let s = &p.plans.string[0];
    assert!(matches!(s.pointer, Some((_, StorePlan::Zoned { digits: 2, .. }))));
    assert!(matches!(&s.sources[..], [lir::StringSource { chars: Chars::Place(_), delimiter: Some(Chars::Literal(space)) }, lir::StringSource { chars: Chars::Literal(twelve), delimiter: None }] if space == &[0x40] && twelve == &[0xF1, 0xF2]));
    let u = &p.plans.unstring[0];
    assert_eq!(u.delimiters, [(false, Chars::Literal(vec![0x6B])), (true, Chars::Literal(vec![0x40]))]);
    assert_eq!(u.into[0].plan, MovePlan::Alnum { image: Image::Bytes, justified: false });
    assert_eq!(u.into[1].plan, MovePlan::Numeric { from: NumericFrom::Zoned, store: StorePlan::Zoned { digits: 4, scale: 0, signed: false, sign: None } });
    let d = u.into[0].delimiter.unwrap();
    assert_eq!((d.found, d.none), (MovePlan::Alnum { image: Image::Bytes, justified: false }, MovePlan::Alnum { image: Image::Figurative, justified: false }));
    assert!(matches!(u.into[0].count, Some((_, StorePlan::Zoned { digits: 1, .. }))));
    let Some((_, tally)) = &u.tallying else { panic!("TALLYING IN") };
    assert!(matches!(tally.store, StorePlan::Refused(a) if symbol(&p, p.abends[a as usize].message) == "TALLYING IN needs a numeric item"));
    let i = &p.plans.inspect[0];
    assert!(matches!(i.tallying[0].counter, Some((_, lir::StepPlan { dmax: 0, store: StorePlan::Zoned { digits: 2, .. } }))));
    assert_eq!(i.tallying[0].pattern, Some(Chars::Literal(vec![0xF0])));
    assert_eq!(i.replacing[0].by, Some(lir::Replacement::Fill(0x40)));
    assert_eq!(i.converting.as_ref().unwrap().table, lir::ConvertTable::Built(vec![(0x81, 0xC1), (0x82, 0xC2)]));
    assert!(matches!(p.plans.inspect[1].converting.as_ref().unwrap().table, lir::ConvertTable::Operands { from: Chars::Literal(_), to: Chars::Place(_) }));
    let selects = p.blocks.iter().filter(|b| matches!(b.ops.last(), Some(Op::String(_) | Op::Unstring(_)))).map(|b| &b.end);
    assert!(selects.into_iter().all(|end| matches!(end, Terminator::Select(arms) if arms.len() == 2)));
}

#[test]
fn a_serial_search_steps_its_index_and_varying_item_and_search_all_matches_keys_to_when_terms() {
    let p = lowered(&program(
        "",
        "       01  N PIC 9 VALUE 3.\n       01  TBL.\n           05 E OCCURS 1 TO 5 DEPENDING ON N\n              ASCENDING KEY IS K INDEXED BY IX.\n              10 K PIC X.\n       01  V PIC 99.\n",
        &[
            line("SEARCH E VARYING V AT END DISPLAY 'NONE'"),
            line("    WHEN K(IX) = 'C' DISPLAY 'C'"),
            line("END-SEARCH"),
            line("SEARCH ALL E WHEN K(IX) = 'D' DISPLAY 'D' END-SEARCH"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    let in_table = p.conds.iter().find_map(|c| if let LirCond::InTable { index, count } = c { Some((*index, count.clone())) } else { None }).unwrap();
    assert_eq!(p.places[in_table.0 as usize].kind, rt::storage::Kind::Index);
    let held = ops(&p).find_map(|op| if let Op::SetCount(t, odo) = op { Some((*t, odo.clone())) } else { None }).unwrap();
    assert!(matches!(held, (t, lir::Odo { max: 5, element: 1, .. }) if in_table.1 == lir::Count::Temp(t)));
    let steps: Vec<&str> = ops(&p).filter_map(|op| if let Op::SetInt { target, .. } = op { Some(symbol(&p, p.places[*target as usize].name)) } else { None }).collect();
    assert_eq!(steps, ["IX", "V"]);
    let a = &p.plans.search_all[0];
    assert_eq!((a.store, a.keys.len(), a.keys[0].ascending, a.keys[0].how), (StorePlan::Index, 1, true, lir::Compare::Alphanumeric));
    let searched = p.blocks.iter().find(|b| matches!(b.ops.last(), Some(Op::SearchAll(_)))).unwrap();
    assert!(matches!(&searched.end, Terminator::Select(arms) if arms.len() == 2));
    assert!(matches!(a.count, lir::Count::Odo(lir::Odo { max: 5, .. })));
}

#[test]
fn under_numcheck_a_serial_search_that_reads_a_tested_index_again_is_refused() {
    let search = |options: &str, data: &str, statement: &str| program(options, data, &[line(statement), line("GOBACK.")].concat());
    let indexed = "SEARCH E WHEN E(IX) = 'C' CONTINUE END-SEARCH";
    let zoned = "       01  N PIC 9 VALUE 3.\n       01  TBL.\n           05 E PIC X OCCURS 1 TO 5 DEPENDING ON N INDEXED BY IX.\n";
    let binary = "       01  N PIC 9 COMP VALUE 3.\n       01  TBL.\n           05 E PIC X OCCURS 1 TO 5 DEPENDING ON N INDEXED BY IX.\n";
    let fixed = "       01  TBL.\n           05 E PIC X OCCURS 5 INDEXED BY IX.\n";
    let varying = "       01  TBL.\n           05 E PIC X OCCURS 5.\n       01  V PIC 9.\n";
    let by_v = "SEARCH E VARYING V WHEN E(V) = 'C' CONTINUE END-SEARCH";
    let e = lower(&compiled(&search("NUMCHECK", varying, by_v))).unwrap_err();
    assert!(matches!(e, LowerError::Unsupported("NUMCHECK of a serial SEARCH's index", _)), "{e}");
    let held = [("NUMCHECK", zoned, indexed), ("NUMCHECK(BIN)", binary, indexed), ("ZONECHECK(MSG)", binary, indexed), ("", zoned, indexed)];
    for (options, data, statement) in held {
        let p = lowered(&search(options, data, statement));
        assert!(ops(&p).any(|op| matches!(op, Op::SetCount(..))), "{options}");
    }
    for (options, data, statement) in [("NUMCHECK", fixed, indexed), ("", varying, by_v)] {
        lowered(&search(options, data, statement));
    }
}

#[test]
fn accept_moves_what_its_source_gives_by_the_receiver_s_plan() {
    let p = lowered(&program(
        "",
        "       01  D6 PIC X(6).\n       01  T PIC 9(8).\n       01  L PIC X(10).\n",
        &[line("ACCEPT D6 FROM DATE"), line("ACCEPT T FROM TIME"), line("ACCEPT L"), line("GOBACK.")].concat(),
    ));
    let accepts: Vec<MovePlan> = ops(&p).filter_map(|op| if let Op::Accept { plan, .. } = op { Some(*plan) } else { None }).collect();
    assert_eq!(accepts[0], MovePlan::Alnum { image: Image::Digits { digits: 6 }, justified: false });
    assert_eq!(accepts[1], MovePlan::Numeric { from: NumericFrom::Value, store: StorePlan::Zoned { digits: 8, scale: 0, signed: false, sign: None } });
    assert_eq!(accepts[2], MovePlan::Alnum { image: Image::Bytes, justified: false });
}

#[test]
fn each_function_s_result_reads_as_the_walker_s_value_does() {
    let data = "       01  A PIC X(9).\n       01  N PIC 9(3) VALUE 7.\n       01  F COMP-2.\n       01  NA PIC N(8).\n";
    let body = [
        "MOVE FUNCTION SUM(N 1) TO A",
        "DISPLAY A",
        "MOVE FUNCTION RANGE(N N) TO A",
        "DISPLAY A",
        "MOVE FUNCTION ORD-MAX(N 1) TO A",
        "DISPLAY A",
        "MOVE FUNCTION FACTORIAL(3) TO A",
        "DISPLAY A",
        "MOVE FUNCTION SQRT(N) TO F",
        "MOVE FUNCTION FORMATTED-DATE(N'YYYYMMDD' 143951) TO NA",
        "MOVE FUNCTION HEX-OF(N) TO A",
        "COMPUTE N = FUNCTION SQRT(4) + 1",
        "COMPUTE N = FUNCTION SUM(N F) + 1",
        "GOBACK.",
    ];
    let source = program("", data, &body.iter().map(|s| line(s)).collect::<String>());
    let out = crate::testing::Harness::source(&source).run(crate::testing::Executor::Interpreter).out;
    assert_eq!(out, "00008    \n0000     \n000000001\n000000000\n");
    let p = lowered(&source);
    let digits = |digits| MovePlan::Alnum { image: Image::Digits { digits }, justified: false };
    let m = moves(&p);
    assert_eq!(m[..4], [digits(5), digits(4), digits(9), digits(30)]);
    assert_eq!(m[4], MovePlan::Float { from: lir::FloatFrom::Float, precision: zarch::hfp::Precision::Long });
    assert_eq!((m[5], m[6]), (MovePlan::National(NationalFrom::Units), MovePlan::Alnum { image: Image::Bytes, justified: false }));
    let f = &p.plans.function;
    let hex = f.iter().find(|f| f.func == lir::Func::HexOf).unwrap();
    assert!(matches!(&hex.args[..], [lir::Argument::Value(Comparand::Operand(LirOperand::Load(q)))] if symbol(&p, p.places[*q as usize].name) == "N"));
    let steps: Vec<Mode> = p.plans.arith.iter().map(|a| a.steps[0].mode).collect();
    assert_eq!(steps, [Mode::Float(numeric::Arith::Compat.float_intermediate()); 2]);
}

#[test]
fn a_wrong_argument_count_abends_with_the_walker_s_message_after_the_arguments() {
    let p = lowered(&program(
        "",
        "       01  N PIC 9(3).\n",
        &[line("COMPUTE N = FUNCTION BYTE-LENGTH(N N)"), line("COMPUTE N = FUNCTION PRESENT-VALUE(1)"), line("COMPUTE N = FUNCTION ANNUITY(1)"), line("GOBACK.")].concat(),
    ));
    let messages: Vec<(usize, &str)> = p.plans.function.iter().map(|f| (f.args.len(), symbol(&p, p.abends[f.arity.unwrap() as usize].message))).collect();
    assert_eq!(
        messages,
        [(0, "FUNCTION BYTE-LENGTH takes one argument"), (1, "FUNCTION PRESENT-VALUE needs a rate and at least one amount"), (1, "FUNCTION ANNUITY takes 2..=2 arguments, not 1")]
    );
}

#[test]
fn all_subscripts_list_a_table_s_elements_or_expand_when_the_function_runs() {
    let data = "       01  T VALUE '010020030'.\n           05 N PIC 9(3) OCCURS 3.\n       01  C PIC 9 VALUE 3.\n       01  D.\n           05 V PIC 9(3) OCCURS 1 TO 5 DEPENDING ON C.\n       01  G VALUE '123456'.\n           05 ROW OCCURS 2.\n              10 CELL PIC 9 OCCURS 3.\n       01  BIG.\n           05 B PIC 9 OCCURS 300.\n       01  A PIC X(9).\n";
    let body = ["MOVE FUNCTION SUM(N(ALL)) TO A", "COMPUTE C = FUNCTION SUM(CELL(ALL ALL))", "COMPUTE C = FUNCTION MAX(V(ALL))", "COMPUTE C = FUNCTION SUM(V(ALL) 1)", "COMPUTE C = FUNCTION SUM(B(ALL))", "GOBACK."];
    let p = lowered(&program("", data, &body.iter().map(|s| line(s)).collect::<String>()));
    let f = &p.plans.function;
    let subscripts = |a: &lir::Argument| match a {
        lir::Argument::Value(Comparand::Operand(LirOperand::Load(q))) => p.places[*q as usize].subscripts.iter().map(|s| if let IntExpr::Const(n) = s.value { n } else { 0 }).collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(f[0].args.iter().map(subscripts).collect::<Vec<_>>(), [[1], [2], [3]]);
    assert_eq!(moves(&p)[0], MovePlan::Alnum { image: Image::Digits { digits: 6 }, justified: false });
    assert_eq!(f[1].args.iter().map(subscripts).collect::<Vec<_>>(), [[1, 1], [1, 2], [1, 3], [2, 1], [2, 2], [2, 3]]);
    let lir::Argument::All { element, all } = &f[2].args[0] else { panic!("{:?}", f[2].args) };
    assert_eq!(symbol(&p, p.places[*element as usize].name), "V");
    assert!(matches!(&all[..], [(0, lir::Count::Odo(lir::Odo { max: 5, check: false, .. }))]));
    assert_eq!(symbol(&p, p.abends[f[2].arity.unwrap() as usize].message), "FUNCTION MAX needs arguments");
    assert_eq!((f[3].args.len(), f[3].arity), (2, None));
    assert!(matches!(&f[4].args[..], [lir::Argument::All { all, .. }] if all[..] == [(0, lir::Count::Fixed(300))]));
    assert_eq!(f[4].arity, None);
}

#[test]
fn the_lir_carries_the_compile_time_when_compiled_gives() {
    let at = lir::CompileTime { seconds: 315_532_800, hundredths: 0, source: lir::TimeSource::SourceDateEpoch };
    let source = program("", "       01  W PIC X(21).\n", &[line("MOVE FUNCTION WHEN-COMPILED TO W"), line("GOBACK.")].concat());
    let compiled = crate::compile_at(syntax::parse(&source).unwrap(), &[], at).unwrap();
    let p = lower(&compiled).unwrap();
    assert_eq!((p.options.when_compiled, round_trip(&p).options.when_compiled), (Some(at), Some(at)));
    assert_eq!((p.plans.function[0].func, moves(&p)[0]), (lir::Func::WhenCompiled, MovePlan::Alnum { image: Image::Bytes, justified: false }));
}

#[test]
fn a_program_without_when_compiled_gives_the_same_module_whenever_it_is_compiled() {
    let source = program("", "       01  W PIC X(21).\n", &[line("MOVE FUNCTION CURRENT-DATE TO W"), line("GOBACK.")].concat());
    let module = |seconds: i64, hundredths: u32| {
        let at = lir::CompileTime { seconds, hundredths, source: lir::TimeSource::Clock };
        let p = lower(&crate::compile_at(syntax::parse(&source).unwrap(), &[], at).unwrap()).unwrap();
        assert_eq!(p.options.when_compiled, None);
        rt::module::write(&[p])
    };
    assert_eq!(module(1_790_510_400, 42), module(315_532_800, 7));
    let stamped = program("", "       01  W PIC X(21).\n", &[line("MOVE FUNCTION WHEN-COMPILED TO W"), line("GOBACK.")].concat());
    let at = |seconds| lir::CompileTime { seconds, hundredths: 0, source: lir::TimeSource::Clock };
    let p = |seconds| lower(&crate::compile_at(syntax::parse(&stamped).unwrap(), &[], at(seconds)).unwrap()).unwrap();
    assert_ne!(rt::module::write(&[p(1)]), rt::module::write(&[p(2)]));
}

#[test]
fn a_floating_point_receiver_makes_the_statement_float_and_numval_and_the_unicode_functions_read_as_the_walker_s() {
    let data = "       01  N PIC 9(3).\n       01  G.\n           05 T PIC 9 OCCURS 3.\n       01  I PIC 9.\n       01  F COMP-2.\n       01  A PIC X(9).\n       01  NA PIC N(4).\n";
    let p = lowered(&program(
        "",
        data,
        &[
            line("COMPUTE N F = T(I) + 1"),
            line("COMPUTE N = T(I) + 1"),
            line("MOVE FUNCTION NUMVAL('12') TO N"),
            line("MOVE FUNCTION ULENGTH(A) TO A"),
            line("MOVE FUNCTION USUBSTR(NA 1 2) TO NA"),
            line("MOVE FUNCTION CONTENT-OF(N) TO A"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    let float = Mode::Float(numeric::Arith::Compat.float_intermediate());
    let steps: Vec<(Mode, usize)> = p.plans.arith[0].steps.iter().map(|s| (s.mode, s.probe.len())).collect();
    assert_eq!(steps, [(float, 0), (float, 0)]);
    assert_eq!((p.plans.arith[1].steps[0].mode, p.plans.arith[1].steps[0].probe.len()), (Mode::Fixed, 1));
    let m = moves(&p);
    assert_eq!(m[0], MovePlan::Numeric { from: NumericFrom::Float, store: StorePlan::Zoned { digits: 3, scale: 0, signed: false, sign: None } });
    assert_eq!((m[1], m[2], m[3]), (MovePlan::Alnum { image: Image::Digits { digits: 9 }, justified: false }, MovePlan::National(NationalFrom::Units), MovePlan::Alnum { image: Image::Digits { digits: 3 }, justified: false }));
}

#[test]
fn the_lowered_program_is_initial_under_the_initial_option_unless_thread_forces_noinitial() {
    let initial = |card: &str, head: &str| {
        let p = lowered(&program(card, "", &line("GOBACK.")).replacen("PROGRAM-ID. T.", &format!("PROGRAM-ID. {head}."), 1));
        (p.initial, p.options.options.initial)
    };
    assert_eq!(initial("", "T"), (false, false));
    assert_eq!(initial("INITIAL", "T"), (true, true));
    assert_eq!(initial("NOINITIAL", "T IS INITIAL"), (true, false));
    assert_eq!(initial("INITIAL,THREAD", "T RECURSIVE"), (false, false));
}

#[test]
fn each_file_carries_the_lengths_its_vlr_setting_checks_a_read_against() {
    let lengths = |card: &str| {
        let source = [
            card,
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            "           SELECT A ASSIGN TO ADD.\n           SELECT B ASSIGN TO BDD.\n           SELECT C ASSIGN TO CDD.\n           SELECT D ASSIGN TO DDD.\n",
            "       DATA DIVISION.\n       FILE SECTION.\n",
            "       FD  A RECORD VARYING IN SIZE FROM 10 TO 80.\n       01  A-20 PIC X(20).\n       01  A-50 PIC X(50).\n",
            "       FD  B RECORD IS VARYING IN SIZE TO 80 CHARACTERS.\n       01  B-20 PIC X(20).\n       01  B-50 PIC X(50).\n",
            "       FD  C RECORD CONTAINS 10 TO 80 CHARACTERS.\n       01  C-20 PIC X(20).\n       01  C-50 PIC X(50).\n",
            "       FD  D RECORDING MODE V.\n       01  D-REC.\n           05 D-N PIC 99.\n           05 D-T PIC X(5) OCCURS 1 TO 10 DEPENDING ON D-N.\n",
            "       WORKING-STORAGE SECTION.\n       PROCEDURE DIVISION.\n",
            &line("GOBACK."),
        ]
        .concat();
        lowered(&source).services.files.iter().map(|f| f.read_lengths).collect::<Vec<_>>()
    };
    assert_eq!(lengths(""), [(20, 50), (20, 50), (20, 50), (7, 52)]);
    assert_eq!(lengths("       CBL VLR(COMPAT)\n"), [(10, 80), (20, 80), (20, 50), (7, 52)]);
}

#[test]
fn inspect_of_a_function_result_tallies_the_value_alone() {
    let p = lowered(&program(
        "",
        "       01  A PIC X(3).\n       01  C PIC 9.\n",
        &[line("INSPECT FUNCTION REVERSE(A) TALLYING C FOR LEADING SPACES"), line("GOBACK.")].concat(),
    ));
    let [plan] = &p.plans.inspect[..] else { panic!("{:?}", p.plans.inspect) };
    assert!(matches!(plan.target, lir::Inspected::Value(LirOperand::Function(0))));
    assert_eq!((plan.tallying.len(), plan.replacing.len(), plan.converting.is_none()), (1, 0, true));
}

#[test]
fn inspect_of_a_national_item_keeps_its_literals_for_rt_to_read_as_national_characters() {
    let p = lowered(&program(
        "",
        "       01  W PIC N(4).\n       01  P PIC N.\n       01  C PIC 9.\n",
        &[
            line("INSPECT W TALLYING C FOR ALL SPACES BEFORE INITIAL N'A'"),
            line("    REPLACING ALL P BY ZERO"),
            line("INSPECT W CONVERTING N'ab' TO N'AB' AFTER INITIAL QUOTE"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    let value = |c: &Chars| match c {
        Chars::Value(LirOperand::Const(k)) => p.consts[*k as usize].clone(),
        other => panic!("{other:?}"),
    };
    use rt::vocab::Figurative::{Quote, Space, Zero};
    let [i, c] = &p.plans.inspect[..] else { panic!("{:?}", p.plans.inspect) };
    assert_eq!(value(i.tallying[0].pattern.as_ref().unwrap()), Const::Figurative(Space));
    assert_eq!(value(&i.tallying[0].bounds[0].value), Const::National(vec![0, 0x41]));
    assert!(matches!(i.replacing[0].pattern, Some(Chars::Place(_))));
    let Some(lir::Replacement::Chars(by)) = &i.replacing[0].by else { panic!("{i:?}") };
    assert_eq!(value(by), Const::Figurative(Zero));
    let converting = c.converting.as_ref().unwrap();
    let lir::ConvertTable::Operands { from, to } = &converting.table else { panic!("{c:?}") };
    assert_eq!((value(from), value(to)), (Const::National(vec![0, 0x61, 0, 0x62]), Const::National(vec![0, 0x41, 0, 0x42])));
    assert_eq!(value(&converting.bounds[0].value), Const::Figurative(Quote));
}

#[test]
fn record_varying_depending_on_names_its_item_and_the_lengths_the_clause_allows() {
    let p = lowered(&with_files(
        "",
        &["    SELECT A ASSIGN TO ADD.", "    SELECT B ASSIGN TO BDD.", "    SELECT C ASSIGN TO CDD."],
        &[
            "FD  A RECORD VARYING FROM 5 TO 60 DEPENDING ON N.",
            "01  A-REC PIC X(80).",
            "FD  B RECORD IS VARYING IN SIZE TO 30 CHARACTERS DEPENDING ON M.",
            "01  B-10 PIC X(10).",
            "01  B-30 PIC X(30).",
            "FD  C.",
            "01  C-REC PIC X(8).",
        ],
        "       01  N PIC 99.\n       01  M PIC 9(4) COMP.\n",
        &["M.", "    WRITE A-REC", "    GOBACK."],
    ));
    let depending: Vec<_> = p.services.files.iter().map(|f| f.depending.map(|d| (symbol(&p, p.places[d.item as usize].name), p.places[d.item as usize].subscripts.len(), d.lengths))).collect();
    assert_eq!(depending, [Some(("N", 0, (5, 60))), Some(("M", 0, (10, 30))), None]);
    verify(&p).unwrap();
}

#[test]
fn release_and_return_take_and_set_an_sd_s_record_length_item_on_both_executors() {
    let source = with_files(
        "",
        &["    SELECT S ASSIGN TO SORTWK."],
        &["SD  S RECORD IS VARYING IN SIZE FROM 1 TO 10 DEPENDING ON N.", "01  S-REC.", "    05 S-KEY PIC X.", "    05 FILLER PIC X(9)."],
        "       01  N PIC 99.\n",
        &[
            "M.",
            "    SORT S ON ASCENDING KEY S-KEY INPUT PROCEDURE IS IN-P",
            "        OUTPUT PROCEDURE IS OUT-P THRU OUT-X",
            "    GOBACK.",
            "IN-P.",
            "    MOVE 3 TO N",
            "    MOVE 'CCCCCCCCCC' TO S-REC",
            "    RELEASE S-REC",
            "    MOVE 5 TO N",
            "    MOVE 'AAAAAAAAAA' TO S-REC",
            "    RELEASE S-REC.",
            "OUT-P.",
            "    RETURN S AT END GO TO OUT-X END-RETURN",
            "    DISPLAY N ' ' S-REC(1:N)",
            "    GO TO OUT-P.",
            "OUT-X.",
            "    EXIT.",
        ],
    );
    let walker = crate::testing::Harness::source(&source).run(crate::testing::Executor::Interpreter);
    let vm = crate::testing::Harness::source(&source).run(crate::testing::Executor::Vm);
    assert_eq!((walker.out.as_str(), &walker.ending), ("05 AAAAA\n03 CCC\n", &Ok(Ending::Goback)), "{}", walker.err);
    assert_eq!((vm.out, vm.ending), (walker.out, walker.ending));
}

#[test]
fn a_move_s_sender_carries_the_test_numcheck_makes_of_it_as_the_walker_decides_it() {
    let data = concat!(
        "       01  Z PIC 999.\n       01  W PIC 999.\n       01  X PIC X(3).\n       01  A PIC X(3).\n       01  P PIC 999 COMP-3.\n",
        "       01  K PIC 9.\n       01  T.\n           05 E PIC X OCCURS 3 INDEXED BY IX.\n",
    );
    let procedure = [
        line("MOVE A TO W"),
        line("MOVE Z TO X"),
        line("MOVE Z TO W"),
        line("MOVE Z TO P"),
        line("MOVE A TO X"),
        line("MOVE 5 TO W"),
        line("SET IX TO Z"),
        line("PERFORM VARYING K FROM Z BY 1 UNTIL K > 3"),
        line("    CONTINUE"),
        line("END-PERFORM"),
        line("GOBACK."),
    ]
    .concat();
    let checks = |options: &str| {
        let p = lowered(&program(options, data, &procedure));
        assert_eq!(ops(&p).filter(|op| matches!(op, Op::Set { .. })).count(), 2, "SET TO and VARYING FROM read as an operand reads");
        ops(&p).filter_map(|op| if let Op::Move { check, .. } = op { Some(*check) } else { None }).collect::<Vec<_>>()
    };
    use SenderCheck::{Integer, Item, None as Untested};
    assert_eq!(checks("NUMCHECK(ZON)"), [Integer, Item, Item, Item, Item, Untested]);
    assert_eq!(checks("NUMCHECK(ZON(LAX))"), [Integer, Untested, Untested, Item, Item, Untested]);
    assert_eq!(checks(""), [Untested; 6]);
}

#[test]
fn write_from_carries_the_test_a_move_makes_of_its_sender() {
    let source = |options: &str| {
        with_files(
            options,
            &["    SELECT OUT-F ASSIGN TO OUTDD."],
            &["FD  OUT-F.", "01  OUT-R PIC 999."],
            "       01  Z PIC 999.\n       01  A PIC X(3).\n",
            &["M.", "    OPEN OUTPUT OUT-F", "    WRITE OUT-R FROM A", "    WRITE OUT-R FROM Z", "    CLOSE OUT-F", "    GOBACK."],
        )
    };
    let checks = |options: &str| {
        let p = lowered(&source(options));
        file_ops(&p).into_iter().filter_map(|(op, _)| if let lir::FileVerb::Write { from: Some(m), .. } = op.verb { Some(m.check) } else { None }).collect::<Vec<_>>()
    };
    assert_eq!(checks("NUMCHECK"), [SenderCheck::Integer, SenderCheck::Item]);
    assert_eq!(checks("NUMCHECK(ZON(LAX))"), [SenderCheck::Integer, SenderCheck::None]);
    assert_eq!(checks(""), [SenderCheck::None; 2]);
}

#[test]
fn under_numcheck_a_condition_name_whose_values_compare_differently_is_refused() {
    let mixed = |options: &str| program(options, "       01  N PIC 9.\n          88 NONE VALUE ZERO SPACE.\n", &[line("IF NONE DISPLAY 'NONE' END-IF"), line("GOBACK.")].concat());
    let e = lower(&compiled(&mixed("NUMCHECK"))).unwrap_err();
    assert!(matches!(e, LowerError::Unsupported("NUMCHECK with a condition-name whose values are of different categories", _)), "{e}");
    lowered(&mixed(""));
    let p = lowered(&program("NUMCHECK", "       01  N PIC 9.\n          88 LOW VALUE 1 THRU 3.\n", &[line("IF LOW DISPLAY 'LOW' END-IF"), line("GOBACK.")].concat()));
    assert!(p.conds.iter().any(|c| matches!(c, LirCond::Name { .. })));
}

#[test]
fn under_numcheck_all_zero_compared_with_an_item_it_may_test_is_refused() {
    let data = "       01  N PIC 9.\n       01  X PIC X.\n";
    let compared = |options: &str, condition: &str| program(options, data, &[line(&format!("IF {condition} DISPLAY 'EQ' END-IF")), line("GOBACK.")].concat());
    for condition in ["N = ALL ZEROS", "ALL ZERO = N"] {
        let e = lower(&compiled(&compared("NUMCHECK", condition))).unwrap_err();
        assert!(matches!(e, LowerError::Unsupported("NUMCHECK with ALL ZERO or ALL NULL compared with a data item it may test", _)), "{condition}: {e}");
        lowered(&compared("", condition));
    }
    lowered(&compared("NUMCHECK", "N = ZERO"));
    lowered(&compared("NUMCHECK", "X = ALL ZEROS"));
}

#[test]
fn a_place_carries_zon_lax_s_tolerance_and_the_compiler_s_removal_of_its_test() {
    use rt::store::LaxRedefinition::{LeadingSpaces, Signed};
    let data = concat!(
        "       01  S1 PIC S999.\n       01  R1 REDEFINES S1.\n           05 R1A PIC 9.\n           05 R1B PIC 99.\n",
        "       01  E1 PIC ZZ,ZZ9.99.\n       01  R3 REDEFINES E1 PIC 9(6).\n       01  W PIC 9(6).\n",
    );
    let lax = |options: &str| {
        let p = lowered(&program(options, data, &[line("COMPUTE W = R1B + R3 + R1A"), line("GOBACK.")].concat()));
        ["R1B", "R3", "R1A", "W"].map(|name| place_named(&p, name)[0].numcheck.lax)
    };
    assert_eq!(lax("NUMCHECK(ZON(LAX))"), [Some(Signed), Some(LeadingSpaces(5)), None, None]);
    assert_eq!(lax("NUMCHECK"), [None; 4], "only ZON(LAX) reads them");
    let constant = |options: &str| {
        let p = lowered(&program(options, "       01  G VALUE ' 12'.\n           05 Z PIC 999.\n       01  W PIC 999.\n", &[line("COMPUTE W = Z + 1"), line("GOBACK.")].concat()));
        place_named(&p, "Z").iter().map(|q| q.numcheck.removed).collect::<Vec<_>>()
    };
    assert_eq!(constant("NUMCHECK"), [true]);
    assert_eq!(constant(""), [false]);
}

#[test]
fn parmcheck_s_buffer_is_in_the_storage_the_lir_describes() {
    let source = |options: &str| program(options, "       01  A PIC X(3).\n", &[line("CALL 'SUB' USING A"), line("GOBACK.")].concat());
    let c = compiled(&source("PARMCHECK(ABD,50)"));
    let p = lowered(&source("PARMCHECK(ABD,50)"));
    assert_eq!(p.storage.parmcheck, c.layout.parmcheck);
    let Some((offset, len)) = p.storage.parmcheck else { panic!("{:?}", p.storage) };
    assert!(len == 50 && offset + len <= p.storage.size, "{offset} {len} {}", p.storage.size);
    assert_eq!(lowered(&source("")).storage.parmcheck, None);
}

#[test]
fn verify_refuses_a_sender_test_numcheck_cannot_make_and_a_parmcheck_buffer_without_parmcheck() {
    let source = |options: &str| program(options, "       01  Z PIC 999.\n       01  W PIC 999.\n", &[line("MOVE Z TO W"), line("GOBACK.")].concat());
    fn first_move(p: &mut Program) -> (&mut LirOperand, &mut SenderCheck) {
        p.blocks.iter_mut().flat_map(|b| &mut b.ops).find_map(|op| if let Op::Move { from, check, .. } = op { Some((from, check)) } else { None }).unwrap()
    }
    let mut constant = lowered(&source("NUMCHECK"));
    *first_move(&mut constant).0 = LirOperand::Const(0);
    assert!(verify(&constant).unwrap_err().contains("a NUMCHECK test Item of a sender that is not a data item"));
    let mut unchecked = lowered(&source(""));
    *first_move(&mut unchecked).1 = SenderCheck::Integer;
    assert!(verify(&unchecked).unwrap_err().contains("without NUMCHECK"));
    let mut buffer = lowered(&source(""));
    buffer.storage.parmcheck = Some((0, 1));
    assert!(verify(&buffer).unwrap_err().contains("without PARMCHECK"));
}

#[test]
fn a_procedure_name_of_digits_is_matched_as_written() {
    let p = lowered(&program(
        "",
        "       01  K PIC 9 VALUE 2.\n",
        &["       MAIN-LINE.\n", &line("GO TO 3 03 DEPENDING ON K."), "       03.\n", &line("DISPLAY '03'."), "       3.\n", &line("DISPLAY '3'.")].concat(),
    ));
    let switch = p.blocks.iter().find_map(|b| if let Terminator::Switch { targets, .. } = &b.end { Some(targets.clone()) } else { None });
    assert_eq!(switch.unwrap(), [paragraph(&p, "3") as u32, paragraph(&p, "03") as u32]);
}

#[test]
fn perform_times_with_a_subscripted_count_sets_its_counter_from_that_element_once() {
    let p = lowered(&program("", TABLE, &[line("PERFORM V (J) TIMES"), line("    CONTINUE"), line("END-PERFORM"), line("GOBACK.")].concat()));
    let counts: Vec<&IntExpr> = ops(&p).filter_map(|op| if let Op::SetTemp(_, n) = op { Some(n) } else { None }).collect();
    let [IntExpr::Item(v)] = counts[..] else { panic!("{counts:?}") };
    let place = &p.places[*v as usize];
    assert_eq!((p.symbols[place.name as usize].as_str(), place.subscripts.len()), ("V", 1));
}

#[test]
fn an_unqualified_paragraph_name_lowers_to_the_one_in_its_own_section() {
    let section = |name: &str| format!("       {name} SECTION.\n       {name}-START.\n{}       P.\n{}       Q.\n{}", line("GO TO P."), line("PERFORM Q."), line("DISPLAY 'Q'."));
    let p = lowered(&program("", "", &[section("S1"), section("S2")].concat()));
    let go_to = |from: usize| p.blocks[p.paragraphs[from].entry as usize].end.clone();
    assert_eq!((go_to(1), go_to(5)), (Terminator::Jump(p.paragraphs[2].entry), Terminator::Jump(p.paragraphs[6].entry)));
    let performed: Vec<(u32, u32)> = p.ranges.iter().map(|r| (r.first, r.last)).collect();
    assert_eq!(performed, [(3, 3), (7, 7)]);
}

#[test]
fn each_statement_with_a_position_starts_once_where_its_first_op_or_terminator_is() {
    let body = [
        "       MAIN.\n",
        &line("MOVE 1 TO N"),
        &line("IF N = 1"),
        &line("    DISPLAY 'A'"),
        &line("ELSE"),
        &line("    DISPLAY 'B'"),
        &line("END-IF"),
        &line("CONTINUE"),
        &line("GO TO P3."),
        &line("DISPLAY 'NEVER'."),
        "       P3.\n",
        &line("STOP RUN."),
    ]
    .concat();
    let source = program("", "       01  N PIC 9.\n", &body);
    let p = lowered(&source);
    let line_of = |text: &str| source.lines().position(|l| l.contains(text)).unwrap() as u32 + 1;
    let mut starts: Vec<(usize, u32, u32)> =
        p.debug.statements.iter().enumerate().flat_map(|(b, s)| s.iter().map(move |&(k, at)| (b, k, at))).map(|(b, k, at)| (b, k, p.debug.positions[at as usize].line)).collect();
    let mut lines: Vec<u32> = starts.iter().map(|s| s.2).collect();
    lines.sort_unstable();
    let mut expected: Vec<u32> = ["MOVE 1", "IF N", "'A'", "'B'", "GO TO", "'NEVER'", "STOP RUN"].map(line_of).to_vec();
    expected.sort_unstable();
    assert_eq!(lines, expected);
    starts.retain(|s| s.2 == line_of("MOVE 1") || s.2 == line_of("GO TO"));
    let entry = p.paragraphs[paragraph(&p, "MAIN")].entry as usize;
    assert_eq!((starts[0].0, starts[0].1), (entry, 0));
    assert!(matches!(p.blocks[entry].ops[0], Op::Move { .. }));
    let (b, k, _) = starts[1];
    assert_eq!(k as usize, p.blocks[b].ops.len());
    assert!(matches!(p.blocks[b].end, Terminator::GoTo(_) | Terminator::Jump(_)), "{:?}", p.blocks[b].end);
}
