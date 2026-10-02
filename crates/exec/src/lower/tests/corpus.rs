//! SEARCH of an OCCURS DEPENDING ON table and literals the code page cannot encode, each lowered
//! and run on both executors.

use super::*;
use crate::testing::{Executor, Harness, Outcome};
use rt::abend::AbendCode;

/// The interpreter's run, which the Harness repeats on the VM and compares, after the VM has run it
/// alone to the end the interpreter reached.
fn on_both(source: &str) -> Outcome {
    let vm = Harness::source(source).run(Executor::Vm);
    let walker = Harness::source(source).run(Executor::Interpreter);
    assert_eq!((&vm.out, &vm.ending), (&walker.out, &walker.ending));
    walker
}

fn abend_of(o: &Outcome) -> (AbendCode, String, u32) {
    match &o.ending {
        Err(a) => (a.code.clone(), a.message.clone(), a.pos.line),
        Ok(e) => panic!("ended {e:?} with output {:?}", o.out),
    }
}

#[test]
fn a_serial_search_holds_its_odo_count_from_the_statement_s_start() {
    let source = program(
        "",
        "       01  N PIC 9 VALUE 5.\n       01  TBL.\n           05 E PIC X OCCURS 1 TO 5 DEPENDING ON N INDEXED BY IX.\n",
        &[
            line("MOVE 'ABCDE' TO TBL"),
            line("MOVE 3 TO N"),
            line("SET IX TO 1"),
            line("SEARCH E VARYING N"),
            line("    AT END DISPLAY 'END ' N"),
            line("    WHEN E(IX) = 'D' DISPLAY 'FOUND ' N"),
            line("END-SEARCH"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let p = lowered(&source);
    let held = ops(&p).find_map(|op| if let Op::SetCount(t, odo) = op { Some((*t, odo.clone())) } else { None }).unwrap();
    assert!(matches!(&held.1, lir::Odo { max: 5, element: 1, check: false, .. }));
    let counts: Vec<_> = p.conds.iter().filter_map(|c| if let LirCond::InTable { count, .. } = c { Some(count.clone()) } else { None }).collect();
    assert_eq!(counts, [lir::Count::Temp(held.0)]);
    assert_eq!(on_both(&source).out, "END 6\n");
}

#[test]
fn a_search_without_an_index_evaluates_its_odo_count_before_it_abends() {
    let data = "       01  G.\n           05 N PIC 9 VALUE 9.\n           05 T PIC X OCCURS 1 TO 3 DEPENDING ON N ASCENDING KEY IS T.\n       01  X PIC 9.\n";
    for all in ["", "ALL "] {
        let body = [line(&format!("SEARCH {all}T WHEN T(X) = 'A' CONTINUE END-SEARCH")), line("GOBACK.")].concat();
        let p = lowered(&program("", data, &body));
        let searched = p.blocks.iter().find(|b| matches!(b.ops.last(), Some(Op::SetCount(..)))).unwrap();
        let Terminator::Abend(a) = searched.end else { panic!("{:?}", searched.end) };
        assert_eq!(symbol(&p, p.abends[a as usize].message), "SEARCH T: the table has no INDEXED BY");
        let (code, message, line) = abend_of(&on_both(&program("", data, &body)));
        assert_eq!((code, message.as_str(), line), (AbendCode::Ironwork, "SEARCH T: the table has no INDEXED BY", 10));
        let (code, message, _) = abend_of(&on_both(&program("SSRANGE", data, &body)));
        assert!(code != AbendCode::Ironwork && message.starts_with("IGZ0007S N = 9"), "{code:?} {message}");
    }
}

#[test]
fn a_literal_the_code_page_cannot_encode_abends_where_the_walker_converts_it() {
    let run = |body: &[&str]| {
        let lines: String = body.iter().map(|s| line(s)).chain([line("DISPLAY 'AFTER'"), line("GOBACK.")]).collect();
        let source = program("", "       01  A PIC X(4) VALUE 'ZZZZ'.\n           88 TICKED VALUE '\u{2713}'.\n", &lines);
        (lowered(&source), on_both(&source))
    };
    let unmappable = "U+2713 has no byte in CCSID 1140";
    let (p, o) = run(&["DISPLAY 'BEFORE'", "MOVE 'A\u{2713}' TO A"]);
    assert_eq!((o.out.as_str(), abend_of(&o)), ("BEFORE\n", (AbendCode::Ironwork, unmappable.to_owned(), 9)));
    let refused: Vec<_> = p.consts.iter().filter_map(|c| if let Const::Refused(a) = c { Some(&p.abends[*a as usize]) } else { None }).collect();
    assert!(matches!(&refused[..], [m] if symbol(&p, m.message) == unmappable && m.at.is_some_and(|at| p.debug.positions[at as usize].line == 9)));

    let (p, o) = run(&["DISPLAY 'B' A '\u{2713}'"]);
    assert_eq!((o.out.as_str(), abend_of(&o).2), ("", 8));
    assert!(matches!(p.plans.display[0].items[..], [DisplayItem::Text(_), DisplayItem::Bytes(_), DisplayItem::Value(LirOperand::Const(c))] if matches!(p.consts[c as usize], Const::Refused(_))));

    let (p, o) = run(&["STRING 'XY' '\u{2713}' DELIMITED BY SIZE INTO A"]);
    assert_eq!(abend_of(&o).1, unmappable);
    assert!(matches!(&p.plans.string[0].sources[..], [s, t] if s.chars == Chars::Literal(vec![0xE7, 0xE8]) && matches!(t.chars, Chars::Value(LirOperand::Const(_)))));

    for body in [&["IF A = '\u{2713}' DISPLAY 'EQUAL' END-IF"][..], &["IF TICKED DISPLAY 'TICKED' END-IF"], &["SET TICKED TO TRUE"], &["MOVE ALL '\u{2713}' TO A"]] {
        let (_, o) = run(body);
        assert_eq!(abend_of(&o), (AbendCode::Ironwork, unmappable.to_owned(), 8), "{body:?}");
    }
}
