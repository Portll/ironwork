use super::*;
use crate::testing::{check_lowering, encoded, line};
use rt::lir::{
    ArithPlan, Base, Collating, Comparand, Cond as LirCond, Const, DisplayItem, Image, IntExpr, Mode, MovePlan, NationalFrom, NumericFrom, Op,
    Operand as LirOperand, Place, Program, SignTest, StorePlan, Terminator,
};
use rt::module::codec::decode_all;

fn program(options: &str, data: &str, procedure: &str) -> String {
    let card = if options.is_empty() { String::new() } else { format!("       CBL {options}\n") };
    format!("{card}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{procedure}")
}

fn compiled(source: &str) -> Compiled {
    crate::compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &[]).unwrap_or_else(|e| panic!("{e:?}"))
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
    ops(p).filter_map(|op| if let Op::Move { plan, .. } = op { Some(*plan) } else { None }).collect()
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
    let enter = p.blocks.iter().find_map(|b| if let Terminator::PerformEnter { range, ret } = b.end { Some((range, ret)) } else { None });
    let (range, ret) = enter.unwrap();
    assert_eq!(range, 0);
    assert_eq!(p.blocks[ret as usize].ops, [Op::Unnest(1)]);
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
        let odo = t.iter().find_map(|q| q.odo.clone()).unwrap();
        assert_eq!((odo.max, odo.element, odo.check), (5, 4, ssrange));
    }
}

#[test]
fn a_receiving_group_holding_its_own_odo_object_is_at_its_maximum_length() {
    let data = "       01  REC.\n           05 CNT PIC 9.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n       01  W PIC X(6).\n";
    let p = lowered(&program("", data, &[line("MOVE W TO REC"), line("MOVE REC TO W"), line("GOBACK.")].concat()));
    let rec = place_named(&p, "REC");
    assert_eq!(rec.len(), 2);
    assert_eq!(rec.iter().filter(|q| q.odo.is_none()).count(), 1);
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
fn a_condition_name_the_walker_cannot_find_again_abends_where_it_is_tested() {
    let p = lowered(&program(
        "",
        "       01  G1.\n           05 F PIC X VALUE 'A'.\n              88 F-A VALUE 'A'.\n       01  G2.\n           05 F PIC X.\n",
        &[line("DISPLAY 'BEFORE'"), line("IF F-A DISPLAY 'A' END-IF"), line("GOBACK.")].concat(),
    ));
    let abend = p.blocks.iter().find_map(|b| if let Terminator::Abend(a) = b.end { Some(a) } else { None }).unwrap();
    assert!(p.symbols[p.abends[abend as usize].message as usize].contains("ambiguous"));
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
    assert!(matches!(all[1], Op::Move { plan: MovePlan::Numeric { from: NumericFrom::Value, .. }, .. }));
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
fn constructs_outside_the_slice_are_refused_by_name() {
    let refused = |body: &str, data: &str| lower(&compiled(&program("", data, &[line(body), line("GOBACK.")].concat()))).unwrap_err();
    assert!(matches!(refused("CALL 'SUB'", ""), LowerError::Unsupported("CALL", _)));
    assert!(matches!(refused("MOVE FUNCTION UPPER-CASE(A) TO A", "       01  A PIC X.\n"), LowerError::Unsupported("FUNCTION", _)));
    assert!(matches!(refused("INSPECT A TALLYING N FOR ALL 'A'", "       01  A PIC X.\n       01  N PIC 9.\n"), LowerError::Unsupported("INSPECT", _)));
    let e = refused("ACCEPT A", "       01  A PIC X.\n");
    assert_eq!(e.to_string(), "lowering: ACCEPT is not lowered yet");
    assert_eq!(syntax::Error::from(e).pos.line, 7);
}

#[test]
fn statements_and_program_features_the_lowering_lacks_are_refused_by_name() {
    let data = "       01  K PIC 9 VALUE 1.\n       01  J PIC 9.\n";
    // A line indented four columns is in area B; any other starts in area A.
    let source = |lines: &[&str]| lines.iter().map(|l| if l.starts_with("    ") { line(l.trim_start()) } else { format!("       {l}\n") }).collect::<String>();
    let named = |lines: &[&str], name: &str| {
        let error = lower(&compiled(&program("", data, &source(lines)))).unwrap_err();
        assert!(matches!(error, LowerError::Unsupported(n, _) if n == name), "{name}: {error}");
    };
    named(&["A.", "    GO TO B C DEPENDING ON K.", "B.", "    GOBACK.", "C.", "    GOBACK."], "GO TO DEPENDING ON");
    named(&["A.", "    ALTER B TO PROCEED TO C.", "B.", "    GO TO.", "C.", "    GOBACK."], "ALTER");
    named(&["A.", "    ENTRY 'ALT'.", "    GOBACK."], "ENTRY");
    named(
        &["A.", "    PERFORM P VARYING K FROM 1 BY 1 UNTIL K > 2", "        AFTER J FROM 1 BY 1 UNTIL J > 2", "    GOBACK.", "P.", "    CONTINUE."],
        "PERFORM VARYING with AFTER",
    );
    named(
        &["DECLARATIVES.", "S SECTION.", "    USE AFTER STANDARD ERROR PROCEDURE ON INPUT.", "P.", "    CONTINUE.", "END DECLARATIVES.", "A.", "    GOBACK."],
        "DECLARATIVES",
    );
    named(&["S1 SECTION 50.", "A.", "    GOBACK."], "an independent segment");
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
    assert_eq!(hows, [lir::Compare::Fixed, lir::Compare::Alphanumeric]);
    let or = p.conds.iter().find_map(|c| if let LirCond::Or(a, b) = c { Some((*a, *b)) } else { None }).unwrap();
    assert_eq!(or, (0, 1));
}
