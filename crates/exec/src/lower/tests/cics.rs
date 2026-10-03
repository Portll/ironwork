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
    let (no_queue, oops) = (paragraph(&p, "NO-QUEUE") as u32, paragraph(&p, "OOPS") as u32);
    let got = commands(&p);
    let names_of: Vec<&str> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(names_of, ["HANDLE CONDITION", "READQ TS", "IGNORE CONDITION", "PUSH HANDLE", "POP HANDLE", "LINK", "RETURN", "ABEND", "XCTL"]);
    let labels = vec![(Condition::QIDERR, Some(no_queue)), (Condition::ERROR, Some(oops)), (Condition::LENGERR, None)];
    assert_eq!(got[0].1.command, Cics::HandleCondition(labels));
    let Cics::ReadqTs { queue, next: false, item: None, numitems: None, record: Record { into, set: None, length } } = &got[1].1.command else { panic!("{:?}", got[1]) };
    assert_eq!((names(&p, queue), names(&p, into), names(&p, length)), (format!("{:?}", Const::Bytes(crate::testing::ebcdic("NOQ"))), "WS-DATA".into(), "WS-LEN".into()));
    let resp = &got[1].1.resp;
    assert_eq!((names(&p, &resp.resp), names(&p, &resp.resp2), resp.nohandle), ("WS-RESP".into(), "WS-R2".into(), false));
    assert_eq!(got[2].1.command, Cics::IgnoreCondition(vec![Condition::LENGERR]));
    let Cics::Link(Transfer { program: Some(Datum::Value(_)), commarea: Some(Datum::Place(_)), length: None }) = got[5].1.command else { panic!("{:?}", got[5]) };
    assert!(got[5].1.resp.nohandle);
    let Cics::Return { transid, commarea, length, channel: None, immediate: false } = &got[6].1.command else { panic!("{:?}", got[6]) };
    assert_eq!(names(&p, commarea), "WS-DATA");
    assert!(matches!((transid, length), (Some(Datum::Value(LirOperand::Const(_))), Some(Datum::Value(LirOperand::Const(_))))));
    assert!(matches!(got[7].1.command, Cics::Abend { abcode: Some(Datum::Value(_)), cancel: true }));
    assert!(matches!(got[8].1.command, Cics::Xctl(_)));
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
fn a_block_the_walker_refuses_as_it_binds_it_keeps_the_walker_s_message() {
    let refused = |block: &str| {
        let p = lowered(&program("", DATA, &[line(block), line("GOBACK."), "       X.\n".to_owned(), line("GOBACK.")].concat()));
        let got = commands(&p);
        let Cics::Refused(why) = got[0].1.command else { panic!("{:?}", got[0]) };
        (got[0].0.to_owned(), symbol(&p, why).to_owned())
    };
    let label = refused("EXEC CICS HANDLE CONDITION ERROR(NOWHERE) END-EXEC");
    assert_eq!(label.0, "HANDLE CONDITION");
    assert!(label.1.starts_with("EXEC CICS HANDLE CONDITION: "), "{}", label.1);
    let two = refused("EXEC CICS HANDLE ABEND LABEL(X) RESET END-EXEC");
    assert_eq!(two, ("HANDLE ABEND".to_owned(), "EXEC CICS HANDLE ABEND takes one of PROGRAM, LABEL, CANCEL and RESET".to_owned()));
}

#[test]
fn a_literal_map_s_symbolic_map_is_written_as_send_map_s_from_and_receive_map_s_into() {
    let data = format!("{DATA}       01  MAP1O PIC X(10).\n       01  MAP1I PIC X(10).\n");
    let body = [
        line("EXEC CICS SEND MAP('map1') END-EXEC"),
        line("EXEC CICS RECEIVE MAP('MAP1') MAPSET('SET1') END-EXEC"),
        line("EXEC CICS SEND MAP('MAP1') MAPONLY END-EXEC"),
        line("EXEC CICS SEND MAP(WS-DATA) END-EXEC"),
        line("EXEC CICS RECEIVE MAP('NOMAP') END-EXEC"),
        line("GOBACK."),
    ]
    .concat();
    let p = lowered(&program("", &data, &body));
    let areas: Vec<String> = commands(&p)
        .iter()
        .map(|(_, c)| match &c.command {
            Cics::SendMap { from, .. } => names(&p, from),
            Cics::ReceiveMap { into, set: None, .. } => names(&p, into),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(areas, ["MAP1O", "MAP1I", "None", "None", "None"]);
}

#[test]
fn a_command_keeps_every_option_an_observer_is_told_in_the_walker_s_order() {
    let data = format!("{DATA}       01  WS-SYS PIC X(4).\n       01  WS-Q PIC X(8).\n");
    let body = [
        line("EXEC CICS WRITEQ TS QUEUE(WS-Q) QNAME(WS-DATA)"),
        line("    FROM(WS-DATA) SYSID(WS-SYS) END-EXEC"),
        line("EXEC CICS WRITE JOURNALNAME('J1') FROM(WS-DATA) END-EXEC"),
        line("EXEC CICS START TRANSID(WS-Q) SYSID(WS-SYS) END-EXEC"),
        line("GOBACK."),
    ]
    .concat();
    let p = lowered(&program("", &data, &body));
    let sinks: Vec<Vec<(String, &str)>> = commands(&p).iter().map(|(_, c)| c.sinks.iter().map(|&(q, s)| (symbol(&p, p.places[q as usize].name).to_owned(), s.kind())).collect()).collect();
    let pair = |name: &str, kind| (name.to_owned(), kind);
    assert_eq!(sinks[0], [pair("WS-Q", "queue-name"), pair("WS-DATA", "queue-name"), pair("WS-SYS", "cics-sysid")]);
    assert_eq!(sinks[1], [pair("WS-DATA", "log")]);
    assert_eq!(sinks[2], [pair("WS-Q", "cics-dynamic-transfer"), pair("WS-SYS", "cics-sysid")]);
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

#[test]
fn handle_abend_keeps_its_label_s_paragraph_and_a_handle_command_s_resp_is_data() {
    let handle_abend = |option: &str| {
        let body = [line(&format!("EXEC CICS HANDLE ABEND {option} END-EXEC")), line("GOBACK."), "       RECOVER.\n".to_owned(), line("GOBACK.")].concat();
        let p = lowered(&program("", DATA, &body));
        commands(&p)[0].1.command.clone()
    };
    assert_eq!(handle_abend("LABEL(RECOVER)"), Cics::HandleAbend { program: None, label: Some(1), reset: false });
    assert!(matches!(handle_abend("PROGRAM('EXITP')"), Cics::HandleAbend { program: Some(Datum::Value(_)), label: None, reset: false }));
    assert_eq!(handle_abend("CANCEL"), Cics::HandleAbend { program: None, label: None, reset: false });
    assert_eq!(handle_abend("RESET"), Cics::HandleAbend { program: None, label: None, reset: true });
    let body = [line("EXEC CICS HANDLE CONDITION ERROR(OOPS) RESP(WS-RESP) END-EXEC"), line("GOBACK."), "       OOPS.\n".to_owned(), line("GOBACK.")].concat();
    let p = lowered(&program("", DATA, &body));
    let got = commands(&p);
    assert_eq!(names(&p, &got[0].1.resp.resp), "WS-RESP");
}

#[test]
fn a_function_option_with_a_floating_point_argument_expression_is_refused() {
    let data = format!("{DATA}       01  F COMP-2 VALUE 2.7.\n");
    let written = |length: &str| program("", &data, &[line("EXEC CICS WRITEQ TS QUEUE('Q1') FROM(WS-DATA)"), line(&format!("  LENGTH({length}) END-EXEC")), line("GOBACK.")].concat());
    for length in ["FUNCTION INTEGER(F * 2)", "FUNCTION ABS(FUNCTION INTEGER(F + 1))"] {
        let e = lower(&compiled(&written(length))).unwrap_err();
        assert!(matches!(e, LowerError::Unsupported("a FUNCTION with a floating-point argument expression as an EXEC CICS option", _)), "{length}: {e}");
    }
    for length in ["FUNCTION INTEGER(F)", "FUNCTION INTEGER(WS-LEN * 2)", "FUNCTION NUMVAL('5.9')"] {
        lowered(&written(length));
    }
}
