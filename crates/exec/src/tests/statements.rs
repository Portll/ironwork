use super::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The Statement events a run of `source` raises under `filter`, as (file, line), with COPY
/// members read from `copy`.
fn statements_in(source: &str, copy: Vec<std::path::PathBuf>, filter: Option<unit::StatementFilter>) -> Vec<(String, u32)> {
    let libraries = syntax::copy::Libraries::new(copy);
    let mut programs = syntax::parse_all_with(source, &libraries).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let observer: unit::Observer<'_> = Box::new(move |e| {
        if let unit::Event::Statement { file, line } = e {
            recorder.borrow_mut().push((file.to_owned(), line));
        }
    });
    let library = unit::Library { programs, copy: libraries, trace_statements: filter, ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ended = compiled.execute_observed(library, files::Dds::default(), None, unit::Clock::Fixed(0, 0), None, &mut out, &mut err, Some(observer));
    assert!(ended.is_ok(), "{ended:?} {}", String::from_utf8_lossy(&err));
    seen.borrow().clone()
}

fn statements(source: &str, filter: Option<unit::StatementFilter>) -> Vec<(String, u32)> {
    statements_in(source, Vec::new(), filter)
}

fn line_of(source: &str, text: &str) -> u32 {
    source.lines().position(|l| l.contains(text)).unwrap_or_else(|| panic!("no line holds {text}")) as u32 + 1
}

fn looped() -> String {
    let body = [
        "       MAIN.\n",
        &line("MOVE 0 TO N"),
        &line("PERFORM P2 2 TIMES"),
        &line("IF N = 2"),
        &line("    DISPLAY 'TWO'"),
        &line("ELSE"),
        &line("    DISPLAY 'NOT'"),
        &line("END-IF"),
        &line("CONTINUE"),
        &line("STOP RUN."),
        "       P2.\n",
        &line("ADD 1 TO N."),
    ]
    .concat();
    program("", "       01  N PIC 9.\n", &body)
}

#[test]
fn each_statement_that_starts_is_told_in_order_continue_among_them() {
    let source = looped();
    let at = |text: &str| (String::new(), line_of(&source, text));
    let expected = [at("MOVE 0"), at("PERFORM P2"), at("ADD 1"), at("ADD 1"), at("IF N"), at("'TWO'"), at("CONTINUE"), at("STOP RUN")];
    assert_eq!(statements(&source, Some(unit::StatementFilter::All)), expected);
}

#[test]
fn only_the_lines_asked_for_are_told_and_none_without_a_filter() {
    let source = looped();
    let add = line_of(&source, "ADD 1");
    let lines = [add, line_of(&source, "'NOT'")].into_iter().collect();
    assert_eq!(statements(&source, Some(unit::StatementFilter::Lines(lines))), vec![(String::new(), add); 2]);
    assert_eq!(statements(&source, None), []);
}

#[test]
fn a_statement_from_a_copy_member_names_the_member() {
    let dir = temp("statement-copy");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("STEPS.cpy"), "           DISPLAY 'COPIED'.\n").unwrap();
    let source = program("", "", &[line("COPY STEPS."), line("GOBACK.")].concat());
    let told = statements_in(&source, vec![dir.clone()], Some(unit::StatementFilter::All));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[0].0.ends_with("STEPS.cpy") && told[0].1 == 1, "{told:?}");
    assert_eq!(told[1], (String::new(), line_of(&source, "GOBACK")));
}
