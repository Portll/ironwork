use super::*;

fn on_vm(source: &str) -> (String, Result<Ending, Abend>) {
    let o = Harness::source(source).run(Executor::Vm);
    (o.out, o.ending)
}

#[test]
fn the_vm_returns_from_a_range_a_go_to_left_and_resumes_after_its_perform() {
    let source = program(
        "",
        "",
        &[
            "       MAIN.\n",
            &line("PERFORM A THRU C."),
            &line("DISPLAY 'BACK'."),
            &line("STOP RUN."),
            "       A.\n",
            &line("DISPLAY 'A'."),
            "       B.\n",
            &line("DISPLAY 'B'."),
            &line("GO TO D."),
            "       C.\n",
            &line("DISPLAY 'C'."),
            "       D.\n",
            &line("DISPLAY 'D'."),
            &line("GO TO C."),
        ]
        .concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "A\nB\nD\nC\nBACK\n");
}

#[test]
fn the_vm_runs_arithmetic_moves_and_tables() {
    let source = program(
        "",
        "       01  T.\n           05 E PIC S9(3)V9 COMP-3 OCCURS 5.\n       01  I PIC 9.\n       01  S PIC S9(5)V9 VALUE 0.\n       01  R PIC Z(4)9.9-.\n",
        &[
            line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5"),
            line("    COMPUTE E(I) ROUNDED = I * 2.55"),
            line("    ADD E(I) TO S"),
            line("END-PERFORM"),
            line("SUBTRACT 50 FROM S"),
            line("MOVE S TO R"),
            line("DISPLAY R"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "   11.6-\n");
}

#[test]
fn the_vm_calls_a_program_of_the_run_unit_and_keeps_its_storage() {
    let source = two_programs(
        "       01  N PIC 9(3) VALUE 5.\n",
        &[line("CALL 'SUB' USING N"), line("CALL 'SUB' USING N"), line("DISPLAY N"), line("STOP RUN.")].concat(),
        "SUB",
        "       WORKING-STORAGE SECTION.\n       01  CALLS PIC 9 VALUE 0.\n       LINKAGE SECTION.\n       01  M PIC 9(3).\n",
        &["       PROCEDURE DIVISION USING M.\n", &line("ADD 1 TO CALLS"), &line("ADD CALLS TO M"), &line("GOBACK.")].concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "008\n");
}

#[test]
fn the_vm_gives_the_interpreter_s_abend_and_position() {
    let source = program("SSRANGE", "       01  T.\n           05 E PIC X OCCURS 3.\n       01  I PIC 9 VALUE 4.\n", &line("DISPLAY E(I)."));
    let (_, ending) = on_vm(&source);
    let abend = ending.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.pos.line), ("U4038", 10));
    assert!(abend.message.starts_with("IGZ0006S subscript 4 of E"), "{}", abend.message);
}

#[test]
#[should_panic(expected = "the VM does not run file I/O yet")]
fn file_statements_stop_the_vm_as_not_run_yet() {
    let source = file_program(
        "           SELECT F ASSIGN TO INFILE.\n",
        "       FD  F.\n       01  REC PIC X(10).\n",
        "",
        &[line("OPEN INPUT F."), line("STOP RUN.")].concat(),
    );
    let _ = on_vm(&source);
}
