use super::*;

fn job(body: &str) -> Result<Job, Error> {
    parse(&format!("//PAYROLL JOB (ACCT),'A PERSON',CLASS=A,MSGCLASS=X\n{body}"))
}

fn steps(j: &Job) -> Vec<&Step> {
    j.items.iter().filter_map(|i| if let Item::Step(s) = i { Some(s) } else { None }).collect()
}

fn refused(body: &str) -> String {
    job(body).unwrap_err().message
}

#[test]
fn steps_dds_and_dispositions_are_read() {
    let j = job(concat!(
        "//* a comment\n",
        "//STEP1    EXEC PGM=PAYCALC,PARM='RUN=1,MODE=''X''' A COMMENT\n",
        "//INPUT    DD DSN=PROD.PAY.MASTER,DISP=SHR\n",
        "//OUTPUT   DD DSN=PROD.PAY.REPORT(JAN),\n",
        "//            DISP=(NEW,CATLG,DELETE),SPACE=(TRK,(5,5)),\n",
        "//            DCB=(RECFM=FB,LRECL=80)\n",
        "//WORK     DD DSN=&&TEMP,DISP=(NEW,PASS)\n",
        "//SYSOUT   DD SYSOUT=*\n",
        "//NOTHING  DD DUMMY\n",
    ))
    .unwrap();
    assert_eq!(j.name, "PAYROLL");
    let s = steps(&j);
    assert_eq!((s[0].name.as_deref(), s[0].pgm.as_str(), s[0].parm.as_deref(), s[0].line), (Some("STEP1"), "PAYCALC", Some("RUN=1,MODE='X'"), 3));
    let dd = |n: &str| &s[0].dds.iter().find(|d| d.name == n).unwrap_or_else(|| panic!("{n} in {:?}", s[0].dds)).parts[0];
    assert_eq!(dd("INPUT").source, Source::Dataset { dsn: "PROD.PAY.MASTER".into(), member: None });
    assert_eq!(dd("INPUT").disp.status, Status::Shr);
    assert_eq!(dd("OUTPUT").source, Source::Dataset { dsn: "PROD.PAY.REPORT".into(), member: Some("JAN".into()) });
    assert_eq!(dd("OUTPUT").disp, Disp { status: Status::New, normal: Some(End::Catlg), abnormal: Some(End::Delete) });
    assert_eq!(dd("WORK").source, Source::Temporary { name: "TEMP".into(), member: None });
    assert_eq!(dd("SYSOUT").source, Source::Sysout);
    assert_eq!(dd("NOTHING").source, Source::Dummy);
}

#[test]
fn in_stream_data_ends_at_its_delimiter() {
    let j = job(concat!(
        "//S1 EXEC PGM=A\n",
        "//SYSIN DD *\n",
        "CARD ONE\n",
        "  CARD TWO    \n",
        "/*\n",
        "//DATA1 DD DATA\n",
        "//NOT A STATEMENT\n",
        "/*\n",
        "//DATA2 DD *,DLM=@@\n",
        "/* KEPT\n",
        "@@\n",
        "//DATA3 DD *\n",
        "LAST\n",
        "//S2 EXEC PGM=B\n",
    ))
    .unwrap();
    let s = steps(&j);
    let lines = |n: &str| match &s[0].dds.iter().find(|d| d.name == n).unwrap().parts[0].source {
        Source::InStream(l) => l.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(lines("SYSIN"), ["CARD ONE", "  CARD TWO"]);
    assert_eq!(lines("DATA1"), ["//NOT A STATEMENT"]);
    assert_eq!(lines("DATA2"), ["/* KEPT"]);
    assert_eq!(lines("DATA3"), ["LAST"]);
    assert_eq!(s[1].pgm, "B");
}

#[test]
fn an_unnamed_dd_is_concatenated_to_the_one_before() {
    let j = job("//S1 EXEC PGM=A\n//IN DD DSN=A.ONE,DISP=SHR\n//   DD DSN=A.TWO,DISP=SHR\n").unwrap();
    assert_eq!(steps(&j)[0].dds[0].parts.len(), 2);
}

#[test]
fn columns_past_71_are_not_read_and_column_72_continues_a_comment() {
    let sequenced = format!("{:<72}{}", "//S1 EXEC PGM=A", "00000100");
    assert_eq!(steps(&job(&format!("{sequenced}\n")).unwrap())[0].pgm, "A");
    let continued = format!("{:<71}X\n//             MORE COMMENT\n//S2 EXEC PGM=B\n", "//S1 EXEC PGM=A  A COMMENT");
    let j = job(&continued).unwrap();
    assert_eq!(steps(&j).iter().map(|s| s.pgm.as_str()).collect::<Vec<_>>(), ["A", "B"]);
}

#[test]
fn a_null_statement_ends_the_job() {
    let j = job("//S1 EXEC PGM=A\n//\n//S2 EXEC PGM=B\n").unwrap();
    assert_eq!(steps(&j).len(), 1);
}

#[test]
fn if_else_endif_and_cond_are_read() {
    let j = job(concat!(
        "//S1 EXEC PGM=A,COND=(4,LT)\n",
        "//CHECK IF (S1.RC = 0 &\n",
        "//         S1.RUN) THEN\n",
        "//S2 EXEC PGM=B\n",
        "// ELSE\n",
        "//S3 EXEC PGM=C\n",
        "// ENDIF\n",
    ))
    .unwrap();
    assert!(matches!(j.items[1], Item::If { line: 3, .. }));
    assert!(matches!(j.items[3], Item::Else { line: 6 }));
    assert!(matches!(j.items[5], Item::EndIf { line: 8 }));
    assert_eq!(steps(&j)[0].cond.tests.len(), 1);
}

#[test]
fn disp_defaults_follow_the_status() {
    let new = Disp::default();
    assert_eq!((new.at_end(false), new.at_end(true)), (End::Delete, End::Delete));
    let old = Disp { status: Status::Old, normal: None, abnormal: None };
    assert_eq!((old.at_end(false), old.at_end(true)), (End::Keep, End::Keep));
    let pass = Disp { status: Status::New, normal: Some(End::Pass), abnormal: None };
    assert_eq!((pass.at_end(false), pass.at_end(true)), (End::Pass, End::Delete));
    let catlg = Disp { status: Status::New, normal: Some(End::Catlg), abnormal: None };
    assert_eq!(catlg.at_end(true), End::Catlg);
}

#[test]
fn names_are_checked() {
    assert!(is_dsn("A.B-C.#$@1") && is_dsn("ABCDEFGH.I"));
    for bad in ["", "A..B", "1A.B", "ABCDEFGHI", "A/B", "../X", "a.b", &"A.".repeat(22)] {
        assert!(!is_dsn(bad), "{bad}");
    }
    assert!(refused("//S1 EXEC PGM=A\n//IN DD DSN=../ETC,DISP=SHR\n").contains("not a data set name"));
    assert!(refused("//S1 EXEC PGM=A\n//IN DD DSN=A.B(../X),DISP=SHR\n").contains("not a member name"));
    assert!(refused("//S1 EXEC PGM=../A\n").contains("not a program name"));
}

#[test]
fn what_is_not_modelled_is_refused_by_name() {
    for (body, message) in [
        ("//S1 EXEC MYPROC\n", "no procedure library holds member MYPROC"),
        ("//S1 EXEC PGM=A,PARM=&P\n", "symbolic parameter &P has no value"),
        ("//S1 EXEC PGM=A\n//IN DD DSN=G.BASE(+1),DISP=(NEW,CATLG)\n", "a generation data group reference is not supported yet"),
        ("//S1 EXEC PGM=A\n//IN DD DSN=*.S0.OUT,DISP=SHR\n", "a backward reference (DSN=*.stepname.ddname) is not supported yet"),
        ("//S1 EXEC PGM=A\n//S1.IN DD DSN=A.B,DISP=SHR\n", "DD S1.IN overrides a procedure step, but the EXEC before it runs a program"),
        ("//S1 EXEC PGM=A\n//IN DD PATH='/u/x'\n", "DD keyword PATH is not supported yet"),
        ("//S1 EXEC PGM=A,PARM='A\n//  B'\n", "a quoted value continued onto the next line is not supported yet"),
    ] {
        assert_eq!(refused(body), message, "{body}");
    }
}

#[test]
fn malformed_jobs_are_refused() {
    assert!(parse("//S1 EXEC PGM=A\n").unwrap_err().message.contains("not a JOB statement"));
    assert!(refused("//IN DD DSN=A.B,DISP=SHR\n").contains("follows no EXEC"));
    assert!(refused("//S1 EXEC PGM=A\nDATA\n").contains("data lines outside"));
    assert!(refused("//S1 EXEC PGM=A\n//IN DD DSN=A,DISP=SHR\n//IN DD DSN=B,DISP=SHR\n").contains("appears twice"));
    assert!(refused("// IF RC = 0 THEN\n//S1 EXEC PGM=A\n").contains("IF without ENDIF"));
    assert!(refused("//S1 EXEC PGM=A\n// ENDIF\n").contains("ENDIF without IF"));
    assert!(refused("//S1 EXEC PGM=A,\n//X EXEC PGM=B\n").contains("continuation line"));
    assert!(refused("//S1 EXEC PGM=A\n//IN DD DSN=A.B,DISP=(SHR,PASS,PASS)\n").contains("abnormal disposition"));
    assert!(refused("//S1 EXEC PGM=A\n//O DD SYSOUT=*,DISP=SHR\n").contains("DISP applies to a data set"));
    let deep = format!("{}//S1 EXEC PGM=A\n{}", "// IF RC = 0 THEN\n".repeat(16), "// ENDIF\n".repeat(16));
    assert!(refused(&deep).contains("more than 15 deep"));
}

#[test]
fn job_cond_takes_plain_tests() {
    assert_eq!(parse("//J JOB ,COND=(8,LE)\n//S1 EXEC PGM=A\n").unwrap().cond.tests.len(), 1);
    assert!(parse("//J JOB COND=(8,LE,S1)\n").unwrap_err().message.contains("tests only"));
    assert!(parse("//J JOB COND=EVEN\n").unwrap_err().message.contains("tests only"));
}

fn library(members: &'static [(&'static str, &'static str)]) -> impl Fn(&[String], &str) -> Result<Option<String>, String> {
    move |order, name| {
        assert_eq!(order, ["MY.PROCLIB"]);
        Ok(members.iter().find(|(n, _)| *n == name).map(|(_, t)| t.to_string()))
    }
}

#[test]
fn an_in_stream_procedure_expands_with_its_symbols_and_overrides() {
    let j = job(concat!(
        "//COPY    PROC HLQ=TEST,OUT=\n",
        "//GEN     EXEC PGM=IEBGENER\n",
        "//SYSUT1  DD DSN=&HLQ..IN,DISP=SHR\n",
        "//SYSUT2  DD DSN=&HLQ..OUT&OUT,DISP=(NEW,CATLG)\n",
        "//SYSIN   DD DUMMY\n",
        "//CHECK   EXEC PGM=VERIFY,COND=(0,NE,GEN),PARM='&HLQ'\n",
        "//        PEND\n",
        "//        SET HLQ=IGNORED\n",
        "//RUN1    EXEC COPY,HLQ=PROD,OUT=2,PARM.CHECK='X',COND.CHECK=(4,LT)\n",
        "//GEN.SYSUT1 DD DISP=OLD\n",
        "//GEN.EXTRA  DD SYSOUT=*\n",
        "//RUN2    EXEC PROC=COPY\n",
        "//SYSIN   DD *\n",
        "CARD\n",
    ))
    .unwrap();
    let s = steps(&j);
    assert_eq!(s.iter().map(|s| s.shown()).collect::<Vec<_>>(), ["RUN1.GEN", "RUN1.CHECK", "RUN2.GEN", "RUN2.CHECK"]);
    let dd = |k: usize, n: &str| &s[k].dds.iter().find(|d| d.name == n).unwrap_or_else(|| panic!("{n}")).parts[0];
    assert_eq!(dd(0, "SYSUT1").source, Source::Dataset { dsn: "PROD.IN".into(), member: None });
    assert_eq!(dd(0, "SYSUT1").disp.status, Status::Old);
    assert_eq!(dd(0, "SYSUT2").source, Source::Dataset { dsn: "PROD.OUT2".into(), member: None });
    assert_eq!(dd(0, "EXTRA").source, Source::Sysout);
    assert_eq!((s[1].parm.as_deref(), s[1].cond.tests[0].code), (Some("X"), 4));
    assert_eq!(dd(2, "SYSUT1").source, Source::Dataset { dsn: "TEST.IN".into(), member: None });
    assert_eq!(dd(2, "SYSIN").source, Source::InStream(vec!["CARD".into()]));
    assert_eq!(s[3].parm.as_deref(), Some("&HLQ"), "a symbol in apostrophes stays as written");
}

#[test]
fn set_values_serve_the_job_and_procedures_without_defaults() {
    let j = job("//  SET ENV=QA\n//NOW PROC\n//S EXEC PGM=A\n//IN DD DSN=&ENV..DATA,DISP=SHR\n// PEND\n//S1 EXEC PGM=B\n//IN DD DSN=&ENV..X,DISP=SHR\n//S2 EXEC NOW\n").unwrap();
    let s = steps(&j);
    assert_eq!(s[0].dds[0].parts[0].source, Source::Dataset { dsn: "QA.X".into(), member: None });
    assert_eq!(s[1].dds[0].parts[0].source, Source::Dataset { dsn: "QA.DATA".into(), member: None });
}

#[test]
fn cataloged_procedures_and_include_members_come_from_the_libraries() {
    let members: &'static [(&'static str, &'static str)] = &[
        ("OUTER", "//OUTER PROC\n//FIRST EXEC PGM=A\n//LATER EXEC INNER\n"),
        ("INNER", "//INNER PROC V=1\n//DEEP EXEC PGM=B,PARM=&V\n// PEND\n"),
        ("COMMON", "//LOG DD SYSOUT=*\n"),
        ("LOOP", "//LOOP PROC\n//S EXEC LOOP\n"),
    ];
    let text = "//J JOB 1\n//LIBS JCLLIB ORDER=MY.PROCLIB\n//S1 EXEC OUTER\n//S2 EXEC PGM=C\n// INCLUDE MEMBER=COMMON\n";
    let j = parse_with(text, &library(members)).unwrap();
    let s = steps(&j);
    assert_eq!(s.iter().map(|s| s.shown()).collect::<Vec<_>>(), ["S1.FIRST", "S1.DEEP", "S2"]);
    assert_eq!(s[1].parm.as_deref(), Some("1"));
    assert_eq!(s[2].dds[0].name, "LOG");
    let e = parse_with("//J JOB 1\n//LIBS JCLLIB ORDER=MY.PROCLIB\n//S1 EXEC LOOP\n", &library(members)).unwrap_err();
    assert!(e.message.contains("nest more than 15 deep"), "{e}");
    let e = parse_with("//J JOB 1\n//LIBS JCLLIB ORDER=MY.PROCLIB\n//S1 EXEC OUTER,NOSUCH=1\n", &library(members)).unwrap_err();
    assert_eq!(e.line, 3);
}

#[test]
fn procedure_mistakes_are_refused() {
    assert!(refused("//P PROC\n//S EXEC PGM=A\n//S1 EXEC P\n").contains("PROC P has no PEND"));
    assert!(refused("//P PROC\n//S EXEC PGM=A\n// PEND\n//S1 EXEC P,PARM.NONE=X\n").contains("has no step NONE"));
    assert!(refused("//P PROC\n//S EXEC PGM=A\n// PEND\n//S1 EXEC P\n//NONE.DD1 DD DUMMY\n").contains("has no step NONE"));
    assert!(refused("//P PROC\n//S EXEC PGM=A,PARM=&X\n// PEND\n//S1 EXEC P\n").contains("P line 3: symbolic parameter &X has no value"));
}
