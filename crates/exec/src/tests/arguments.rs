use super::*;
use crate::testing::ebcdic;
use std::cell::RefCell;
use std::rc::Rc;

const SUBPROGRAM: &str = "       IDENTIFICATION DIVISION.
       PROGRAM-ID. SUB.
       DATA DIVISION.
       LINKAGE SECTION.
       01  QTY     PIC 9(5).
       01  NAME    PIC X(8).
       01  MISSING PIC X(4).
       PROCEDURE DIVISION USING QTY NAME MISSING.
           DISPLAY 'NAME ' NAME
           IF ADDRESS OF MISSING = NULL
              DISPLAY 'OMITTED'
           END-IF
           ADD 1 TO QTY
           DISPLAY 'QTY ' QTY
           EXIT PROGRAM.
           DISPLAY 'AFTER EXIT'.
";

fn arguments(qty: &str) -> Vec<Option<Vec<u8>>> {
    vec![Some(ebcdic(qty)), Some(ebcdic("ALICE   ")), None]
}

#[test]
fn a_subprogram_reads_its_arguments_as_its_using_items_and_exit_program_returns() {
    for (executor, name) in [(Executor::Interpreter, "interpreter"), (Executor::Vm, "vm")] {
        let o = Harness::source(SUBPROGRAM).arguments(arguments("00041")).run(executor);
        assert!(o.ending.is_ok(), "{:?} {}", o.ending, o.err);
        assert_eq!(o.out, "NAME ALICE   \nOMITTED\nQTY 00042\n", "{name}");
    }
}

#[test]
fn an_argument_that_breaks_its_picture_ends_the_subprogram_where_it_is_used() {
    for (executor, name) in [(Executor::Interpreter, "interpreter"), (Executor::Vm, "vm")] {
        let o = Harness::source(SUBPROGRAM).arguments(arguments("AB*DE")).run(executor);
        let abend = o.ending.expect_err("a non-numeric QTY is a data exception at the ADD");
        assert_eq!(abend.code.as_str(), "S0C7", "{name}");
        assert_eq!(o.out, "NAME ALICE   \nOMITTED\n", "{name}");
    }
}

#[test]
fn every_argument_is_input_to_the_run() {
    let mut programs = syntax::parse_all_with(SUBPROGRAM, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let observer: unit::Observer<'_> = Box::new(move |e| {
        if let unit::Event::Sink { operand, input, .. } = e {
            recorder.borrow_mut().push((operand.to_owned(), input));
        }
    });
    let library = unit::Library { programs, trace_input: true, ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut kept = None;
    let ended = compiled.execute_with_arguments(library, files::Dds::default(), None, unit::Clock::Fixed(0, 0), None, &mut out, &mut err, Some(observer), &arguments("00041"), &mut kept);
    assert!(ended.is_ok(), "{ended:?} {}", String::from_utf8_lossy(&err));
    assert_eq!(kept.map(|k| k.arguments), Some(vec![Some(ebcdic("00042")), Some(ebcdic("ALICE   ")), None]), "the caller sees the ADD in QTY");
    let seen = seen.borrow();
    let input_of = |text: &str| seen.iter().find(|(operand, _)| operand.contains(text)).unwrap_or_else(|| panic!("no sink shows {text}: {seen:?}")).1;
    assert_eq!(input_of("ALICE"), Some(true));
    assert_eq!(input_of("00042"), Some(true));
    assert_eq!(input_of("OMITTED"), Some(false));
}

/// A CALL no member of the program libraries answers by name finds the source file whose
/// PROGRAM-ID it is, a member of the name coming first (assumption C441).
#[test]
fn a_call_finds_a_program_id_in_a_library_file_of_another_name() {
    let library = temp("program-id-library");
    std::fs::create_dir_all(&library).unwrap();
    let callee = |id: &str, says: &str| format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       PROCEDURE DIVISION.\n           DISPLAY '{says}'.\n           GOBACK.\n");
    std::fs::write(library.join("payroll-batch.cbl"), callee("PAYCALC", "BY PROGRAM-ID")).unwrap();
    std::fs::write(library.join("another.cbl"), callee("SUBA", "SUBA BY PROGRAM-ID")).unwrap();
    std::fs::write(library.join("SUBA.cbl"), callee("SUBA", "SUBA BY MEMBER")).unwrap();
    std::fs::write(library.join("notes.txt"), callee("NOTES", "NOT A LIBRARY MEMBER")).unwrap();
    let main = program("", "", &[line("CALL 'PAYCALC'"), line("CALL 'SUBA'"), line("CALL 'NOTES'"), line("GOBACK.")].concat());
    let walker = Harness::source(&main).dirs(vec![library.clone()]).run(Executor::Interpreter);
    let vm = Harness::source(&main).dirs(vec![library.clone()]).run(Executor::Vm);
    std::fs::remove_dir_all(&library).unwrap();
    assert_eq!(walker.out, "BY PROGRAM-ID\nSUBA BY MEMBER\n");
    assert_eq!(walker.ending.as_ref().map_err(|a| a.code.clone()), Err(AbendCode::Ironwork));
    assert_eq!((&vm.out, &vm.ending), (&walker.out, &walker.ending));
}
