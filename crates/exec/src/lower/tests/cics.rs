use super::*;
use rt::abend::AbendCode;
use rt::cics::{Cics, CicsCommand, Condition, Datum, Record, Transfer};

fn commands(p: &Program) -> Vec<(&str, &CicsCommand)> {
    let ids = ops(p).filter_map(|op| if let Op::Cics(c) = op { Some(*c as usize) } else { None });
    ids.map(|c| (symbol(p, p.services.cics[c].name), &p.services.cics[c])).collect()
}

fn names(p: &Program, d: &Option<Datum>) -> String {
    match d {
        Some(Datum::Place(q)) => symbol(p, p.places[*q as usize].name).to_owned(),
        Some(Datum::Value(LirOperand::Const(c))) => format!("{:?}", p.consts[*c as usize]),
        other => format!("{other:?}"),
    }
}

const DATA: &str = "       01  WS-DATA PIC X(8).\n       01  WS-LEN PIC S9(4) COMP VALUE 8.\n       01  WS-RESP PIC S9(8) COMP.\n       01  WS-R2 PIC S9(8) COMP.\n";

#[test]
fn each_block_is_one_op_whose_command_binds_as_the_walker_binds_it_and_handle_keeps_its_paragraphs() {
    let body = [
        "       MAIN-LINE.\n",
        &line("EXEC CICS HANDLE CONDITION QIDERR(NO-QUEUE) ERROR(OOPS)"),
        &line("    LENGERR END-EXEC"),
        &line("EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC"),
        &line("EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA) LENGTH(WS-LEN)"),
        &line("    RESP(WS-RESP) RESP2(WS-R2) END-EXEC"),
        &line("EXEC CICS IGNORE CONDITION LENGERR END-EXEC"),
        &line("EXEC CICS PUSH HANDLE END-EXEC"),
        &line("EXEC CICS POP HANDLE END-EXEC"),
        &line("EXEC CICS LINK PROGRAM('SUBP') COMMAREA(WS-DATA) NOHANDLE"),
        &line("    END-EXEC"),
        &line("EXEC CICS RETURN TRANSID('ABCD') COMMAREA(WS-DATA)"),
        &line("    LENGTH(8) END-EXEC."),
        "       NO-QUEUE.\n",
        &line("DISPLAY 'NONE'."),
        "       OOPS.\n",
        &line("EXEC CICS ABEND ABCODE('XY12') CANCEL END-EXEC."),
        "       RECOVER.\n",
        &line("EXEC CICS XCTL PROGRAM('LASTP') END-EXEC."),
    ]
    .concat();
    let p = lowered(&program("", DATA, &body));
    let (no_queue, oops, recover) = (paragraph(&p, "NO-QUEUE") as u32, paragraph(&p, "OOPS") as u32, paragraph(&p, "RECOVER") as u32);
    let got = commands(&p);
    let names_of: Vec<&str> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(names_of, ["HANDLE CONDITION", "HANDLE ABEND", "READQ TS", "IGNORE CONDITION", "PUSH HANDLE", "POP HANDLE", "LINK", "RETURN", "ABEND", "XCTL"]);
    let labels = vec![(Condition::QIDERR, Some(no_queue)), (Condition::ERROR, Some(oops)), (Condition::LENGERR, None)];
    assert_eq!(got[0].1.command, Cics::HandleCondition(labels));
    assert_eq!(got[1].1.command, Cics::HandleAbend { program: false, label: Some(recover), reset: false });
    let Cics::ReadqTs { queue, next: false, item: None, numitems: None, record: Record { into, set: None, length } } = &got[2].1.command else { panic!("{:?}", got[2]) };
    assert_eq!((names(&p, queue), names(&p, into), names(&p, length)), (format!("{:?}", Const::Bytes(crate::testing::ebcdic("NOQ"))), "WS-DATA".into(), "WS-LEN".into()));
    let resp = &got[2].1.resp;
    assert_eq!((names(&p, &resp.resp), names(&p, &resp.resp2), resp.nohandle), ("WS-RESP".into(), "WS-R2".into(), false));
    assert_eq!(got[3].1.command, Cics::IgnoreCondition(vec![Condition::LENGERR]));
    let Cics::Link(Transfer { program: Some(Datum::Value(_)), commarea: Some(Datum::Place(_)), length: None }) = got[6].1.command else { panic!("{:?}", got[6]) };
    assert!(got[6].1.resp.nohandle);
    let Cics::Return { transid, commarea, length } = &got[7].1.command else { panic!("{:?}", got[7]) };
    assert_eq!(names(&p, commarea), "WS-DATA");
    assert!(matches!((transid, length), (Some(Datum::Value(LirOperand::Const(_))), Some(Datum::Value(LirOperand::Const(_))))));
    assert!(matches!(got[8].1.command, Cics::Abend { abcode: Some(Datum::Value(_)), cancel: true }));
    assert!(matches!(got[9].1.command, Cics::Xctl(_)));
    // Each op returns its own transfer, so no terminator follows it.
    assert_eq!(end_of(&p, "MAIN-LINE"), Terminator::Jump(p.paragraphs[no_queue as usize].entry));
}

#[test]
fn a_command_ironwork_does_not_carry_out_lowers_to_its_op_and_abends_when_reached() {
    let p = lowered(&program("", DATA, &[line("EXEC CICS START TRANSID('ABCD') END-EXEC"), line("GOBACK.")].concat()));
    let got = commands(&p);
    assert_eq!((got.len(), got[0].0, &got[0].1.command), (1, "START", &Cics::Unsupported));
}

#[test]
fn a_handle_label_that_names_no_procedure_is_refused() {
    let source = program("", DATA, &[line("EXEC CICS HANDLE CONDITION ERROR(NOWHERE) END-EXEC"), line("GOBACK.")].concat());
    let error = lower(&compiled(&source)).unwrap_err();
    assert!(matches!(error, LowerError::Unsupported("a HANDLE label that names no procedure", _)), "{error}");
}

#[test]
fn exec_dli_ends_its_block_in_the_walker_s_exec_abend() {
    let p = lowered(&program("", "       01  AREA1 PIC X(80).\n", &[line("EXEC DLI GN SEGMENT(ROOT) INTO(AREA1) END-EXEC"), line("GOBACK.")].concat()));
    let Terminator::Abend(a) = p.blocks[0].end else { panic!("{:?}", p.blocks[0].end) };
    let text = &p.abends[a as usize];
    assert_eq!((&text.code, symbol(&p, text.message), text.at), (&AbendCode::Exec, "EXEC DLI GN was reached: ironwork for COBOL checks EXEC statements but does not run them yet", None));
}

#[test]
fn a_handle_label_two_sections_have_is_the_one_in_the_handle_command_s_section() {
    let section = |name: &str| format!("       {name} SECTION.\n{}       OOPS.\n{}", line("EXEC CICS HANDLE CONDITION ERROR(OOPS) END-EXEC."), line("GOBACK."));
    let p = lowered(&program("", DATA, &[section("S1"), section("S2")].concat()));
    let labels: Vec<Cics> = commands(&p).into_iter().map(|(_, c)| c.command.clone()).collect();
    assert_eq!(labels, [Cics::HandleCondition(vec![(Condition::ERROR, Some(1))]), Cics::HandleCondition(vec![(Condition::ERROR, Some(3))])]);
}
