//! `ironwork job`: steps run in order on a data set directory; dispositions create, keep and
//! delete; COND and IF choose the steps; an abend bypasses what follows; what ironwork does not
//! run is refused before the first step.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-job-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

fn cobol(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

/// Copies IN to OUT upper-cased, DISPLAYs the count, and sets RETURN-CODE to the first line of
/// SYSIN when there is one.
fn upcase(dir: &Path) {
    let text = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. UPCASE.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT IN-FILE ASSIGN TO IN.",
        "    SELECT OUT-FILE ASSIGN TO OUT.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "FD IN-FILE.",
        "01 IN-REC PIC X(20).",
        "FD OUT-FILE.",
        "01 OUT-REC PIC X(20).",
        "WORKING-STORAGE SECTION.",
        "01 WS-EOF PIC X VALUE 'N'.",
        "01 WS-N PIC 9(3) VALUE 0.",
        "01 WS-RC PIC 9(4) VALUE 0.",
        "PROCEDURE DIVISION.",
        "    ACCEPT WS-RC.",
        "    OPEN INPUT IN-FILE OUTPUT OUT-FILE.",
        "    PERFORM UNTIL WS-EOF = 'Y'",
        "        READ IN-FILE AT END MOVE 'Y' TO WS-EOF",
        "        NOT AT END",
        "            MOVE FUNCTION UPPER-CASE(IN-REC) TO OUT-REC",
        "            WRITE OUT-REC",
        "            ADD 1 TO WS-N",
        "        END-READ",
        "    END-PERFORM.",
        "    CLOSE IN-FILE OUT-FILE.",
        "    DISPLAY 'COPIED ' WS-N.",
        "    MOVE WS-RC TO RETURN-CODE.",
        "    GOBACK.",
    ]);
    fs::write(dir.join("lib/UPCASE.cbl"), text).unwrap();
}

/// Divides by a zero it reads from SYSIN, a program check.
fn divide(dir: &Path) {
    let text = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. DIVIDE.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01 WS-D PIC S9(3) COMP-3 VALUE 0.",
        "01 WS-Q PIC S9(3) COMP-3.",
        "PROCEDURE DIVISION.",
        "    DIVIDE 10 BY WS-D GIVING WS-Q.",
        "    GOBACK.",
    ]);
    fs::write(dir.join("lib/DIVIDE.cbl"), text).unwrap();
}

fn job(dir: &Path, jcl: &str) -> Output {
    job_with(dir, jcl, &[])
}

fn job_with(dir: &Path, jcl: &str, extra: &[&str]) -> Output {
    let path = dir.join("job.jcl");
    fs::write(&path, format!("//TESTJOB JOB (1),'T',CLASS=A\n{jcl}")).unwrap();
    Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .args(["job", path.to_str().unwrap(), "--datasets"])
        .arg(format!("{}:text", dir.join("data").display()))
        .args(["-L", dir.join("lib").to_str().unwrap(), "--clock", "2026-01-01T00:00:00"])
        .args(extra)
        .output()
        .unwrap()
}

fn log(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn steps_pass_data_sets_along_and_dispositions_apply() {
    let dir = temp("chain");
    upcase(&dir);
    fs::write(dir.join("data/IN.NAMES"), "alpha\nbeta\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//MAKE     EXEC PGM=IEBGENER\n",
            "//SYSUT1   DD DSN=IN.NAMES,DISP=SHR\n",
            "//SYSUT2   DD DSN=&&COPY,DISP=(NEW,PASS)\n",
            "//SYSPRINT DD SYSOUT=*\n",
            "//SYSIN    DD DUMMY\n",
            "//UP       EXEC PGM=UPCASE\n",
            "//IN       DD DSN=&&COPY,DISP=(OLD,DELETE)\n",
            "//OUT      DD DSN=OUT.NAMES,DISP=(NEW,CATLG,DELETE)\n",
            "//SCRATCH  DD DSN=WORK.GONE,DISP=(NEW,DELETE)\n",
            "//SYSIN    DD *\n",
            "0004\n",
            "/*\n",
            "//CLEAN    EXEC PGM=IEFBR14\n",
            "//OLD      DD DSN=IN.NAMES,DISP=(OLD,DELETE)\n",
        ),
    );
    assert_eq!(o.status.code(), Some(4), "{}", log(&o));
    assert_eq!(fs::read_to_string(dir.join("data/OUT.NAMES")).unwrap(), "ALPHA\nBETA\n");
    assert!(!dir.join("data/IN.NAMES").exists() && !dir.join("data/WORK.GONE").exists());
    assert_eq!(String::from_utf8_lossy(&o.stdout), "COPIED 002\n");
    let l = log(&o);
    for want in ["MAKE PGM=IEBGENER RC=0000", "UP PGM=UPCASE RC=0004", "CLEAN PGM=IEFBR14 RC=0000"] {
        assert!(l.contains(want), "{want} in {l}");
    }
}

#[test]
fn cond_and_if_choose_the_steps() {
    let dir = temp("cond");
    upcase(&dir);
    fs::write(dir.join("data/IN.X"), "x\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//S1 EXEC PGM=UPCASE\n",
            "//IN DD DSN=IN.X,DISP=SHR\n",
            "//OUT DD DUMMY\n",
            "//SYSIN DD *\n",
            "0008\n",
            "//S2 EXEC PGM=IEFBR14,COND=(4,LT)\n",
            "//S3 EXEC PGM=IEFBR14,COND=(12,LT,S1)\n",
            "// IF (S1.RC >= 8) THEN\n",
            "//S4 EXEC PGM=IEFBR14\n",
            "// ELSE\n",
            "//S5 EXEC PGM=IEFBR14\n",
            "// ENDIF\n",
        ),
    );
    let l = log(&o);
    assert!(l.contains("S2 PGM=IEFBR14 BYPASSED"), "{l}");
    assert!(l.contains("S3 PGM=IEFBR14 RC=0000"), "{l}");
    assert!(l.contains("S4 PGM=IEFBR14 RC=0000"), "{l}");
    assert!(l.contains("S5 PGM=IEFBR14 BYPASSED: its IF branch is not taken"), "{l}");
    assert_eq!(o.status.code(), Some(8));
}

#[test]
fn an_abend_bypasses_later_steps_except_even_and_abend_tests() {
    let dir = temp("abend");
    divide(&dir);
    let o = job(
        &dir,
        concat!(
            "//S1 EXEC PGM=DIVIDE\n",
            "//S2 EXEC PGM=IEFBR14\n",
            "//S3 EXEC PGM=IEFBR14,COND=EVEN\n",
            "//S4 EXEC PGM=IEFBR14,COND=ONLY\n",
            "// IF ABEND THEN\n",
            "//S5 EXEC PGM=IEFBR14\n",
            "// ENDIF\n",
            "//S6 EXEC PGM=NOSUCH,COND=EVEN\n",
        ),
    );
    let l = log(&o);
    assert!(l.contains("S1 PGM=DIVIDE ABEND S0CB"), "{l}");
    assert!(l.contains("S2 PGM=IEFBR14 BYPASSED: an earlier step abended"), "{l}");
    for ran in ["S3", "S4", "S5"] {
        assert!(l.contains(&format!("{ran} PGM=IEFBR14 RC=0000")), "{ran}: {l}");
    }
    assert!(l.contains("S6 PGM=NOSUCH ABEND S806"), "{l}");
    assert_eq!(o.status.code(), Some(16));
}

#[test]
fn a_missing_data_set_is_a_jcl_error_that_ends_the_job() {
    let dir = temp("jclerr");
    fs::write(dir.join("data/EXISTS.ALREADY"), "").unwrap();
    let o = job(&dir, "//S1 EXEC PGM=IEFBR14\n//IN DD DSN=NOT.THERE,DISP=SHR\n//S2 EXEC PGM=IEFBR14\n//NEW DD DSN=EXISTS.ALREADY,DISP=NEW\n");
    let l = log(&o);
    assert!(l.contains("S1 PGM=IEFBR14 JCL ERROR: DD IN: NOT.THERE was not found; the job ends"), "{l}");
    assert!(l.contains("S2 PGM=IEFBR14 BYPASSED: the job ended"), "{l}");
    assert_eq!(o.status.code(), Some(16));
}

#[test]
fn members_of_a_partitioned_data_set_are_files_in_its_directory() {
    let dir = temp("pds");
    fs::write(dir.join("data/IN.X"), "one\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//S1 EXEC PGM=IEBGENER\n",
            "//SYSUT1 DD DSN=IN.X,DISP=SHR\n",
            "//SYSUT2 DD DSN=MY.PDS(FIRST),DISP=(NEW,CATLG)\n",
            "//SYSIN DD DUMMY\n",
            "//S2 EXEC PGM=IEBGENER\n",
            "//SYSUT1 DD DSN=MY.PDS(FIRST),DISP=SHR\n",
            "//       DD DSN=IN.X,DISP=SHR\n",
            "//SYSUT2 DD DSN=MY.PDS(SECOND),DISP=OLD\n",
            "//SYSIN DD DUMMY\n",
        ),
    );
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    assert_eq!(fs::read_to_string(dir.join("data/MY.PDS/SECOND")).unwrap(), "one\none\n");
}

#[test]
fn what_ironwork_does_not_run_is_refused_before_any_step() {
    let dir = temp("refuse");
    let o = job(&dir, "//S1 EXEC PGM=IEFBR14\n//NEW DD DSN=MADE.EARLY,DISP=(NEW,CATLG)\n//S2 EXEC PGM=ICETOOL\n//S4 EXEC PGM=IDCAMS\n//SYSIN DD *\n  LISTCAT ALL\n");
    assert_eq!(o.status.code(), Some(2));
    let l = log(&o);
    assert!(l.contains("PGM=ICETOOL is not supported yet") && l.contains("IDCAMS: the IDCAMS command LISTCAT is not supported yet"), "{l}");
    assert!(!dir.join("data/MADE.EARLY").exists());
    let o = job(&dir, "//S1 EXEC MYPROC\n");
    assert_eq!(o.status.code(), Some(2));
    assert!(log(&o).contains("job.jcl:2: no procedure library holds member MYPROC"), "{}", log(&o));
}

#[test]
fn procedures_from_jcllib_and_proclib_run_with_their_overrides() {
    let dir = temp("procs");
    upcase(&dir);
    fs::create_dir_all(dir.join("data/SITE.PROCLIB")).unwrap();
    fs::create_dir_all(dir.join("sys")).unwrap();
    fs::write(dir.join("data/IN.NAMES"), "gamma\n").unwrap();
    fs::write(dir.join("data/SITE.PROCLIB/UPPROC"), concat!(
        "//UPPROC  PROC OUT=OUT.DEFAULT\n",
        "//UP      EXEC PGM=UPCASE\n",
        "//IN      DD DSN=IN.NAMES,DISP=SHR\n",
        "//OUT     DD DSN=&OUT,DISP=(NEW,CATLG)\n",
        "//SYSIN   DD DUMMY\n",
        "//AFTER   EXEC TIDY\n",
    )).unwrap();
    fs::write(dir.join("sys/TIDY.jcl"), "//TIDY PROC\n//GONE EXEC PGM=IEFBR14,COND=(0,NE,UP)\n").unwrap();
    let o = job_with(
        &dir,
        concat!(
            "//LIBS    JCLLIB ORDER=SITE.PROCLIB\n",
            "//        SET TARGET=OUT.SET\n",
            "//RUN     EXEC UPPROC,OUT=&TARGET\n",
            "//UP.SYSIN DD *\n",
            "0002\n",
            "// IF (RUN.UP.RC = 2) THEN\n",
            "//YES     EXEC PGM=IEFBR14\n",
            "// ENDIF\n",
        ),
        &["--proclib", dir.join("sys").to_str().unwrap()],
    );
    let l = log(&o);
    assert_eq!(o.status.code(), Some(2), "{l}");
    assert_eq!(fs::read_to_string(dir.join("data/OUT.SET")).unwrap(), "GAMMA\n");
    assert!(l.contains("RUN.UP PGM=UPCASE RC=0002") && l.contains("RUN.GONE PGM=IEFBR14 BYPASSED: COND=(0,NE,UP) is true") && l.contains("YES PGM=IEFBR14 RC=0000"), "{l}");
}

#[test]
fn idcams_deletes_defines_and_copies_with_its_condition_codes() {
    let dir = temp("idcams");
    fs::write(dir.join("data/SRC.DATA"), "one\ntwo\n").unwrap();
    fs::write(dir.join("data/OLD.DATA"), "x\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//CLEAN    EXEC PGM=IDCAMS\n",
            "//SYSPRINT DD SYSOUT=*\n",
            "//SYSIN    DD *\n",
            "  DELETE (OLD.DATA NEVER.THERE) PURGE\n",
            "  IF LASTCC = 8 THEN SET MAXCC = 0\n",
            "  DEFINE CLUSTER (NAME(NEW.KSDS) INDEXED KEYS(4 0) -\n",
            "         RECORDSIZE(20 20))\n",
            "  REPRO INFILE(IN) OUTDATASET(NEW.KSDS)\n",
            "/*\n",
            "//IN       DD DSN=SRC.DATA,DISP=SHR\n",
            "//FAIL     EXEC PGM=IDCAMS\n",
            "//SYSPRINT DD SYSOUT=*\n",
            "//SYSIN    DD *\n",
            "  REPRO INDATASET(SRC.DATA) OUTDATASET(NOT.DEFINED)\n",
            "  DELETE SRC.DATA\n",
            "/*\n",
        ),
    );
    let l = log(&o);
    assert!(l.contains("CLEAN PGM=IDCAMS RC=0000") && l.contains("FAIL PGM=IDCAMS RC=0012"), "{l}");
    assert_eq!(o.status.code(), Some(12));
    assert!(!dir.join("data/OLD.DATA").exists() && !dir.join("data/SRC.DATA").exists());
    assert_eq!(fs::read_to_string(dir.join("data/NEW.KSDS")).unwrap(), "one\ntwo\n");
    let out = String::from_utf8_lossy(&o.stdout);
    for want in ["IDC0550I ENTRY (A) OLD.DATA DELETED", "IDC3012I ENTRY NEVER.THERE NOT FOUND", "IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS 8", "IDC0002I IDCAMS PROCESSING COMPLETE. MAXIMUM CONDITION CODE WAS 0", "MAXIMUM CONDITION CODE WAS 12"] {
        assert!(out.contains(want), "{want} in {out}");
    }
}

#[test]
fn a_job_is_equivalent_to_production_when_its_data_sets_match() {
    let dir = temp("equiv");
    upcase(&dir);
    fs::write(dir.join("data/IN.NAMES"), "alpha\nbeta\n").unwrap();
    fs::create_dir_all(dir.join("prod")).unwrap();
    let jcl = "//UP EXEC PGM=UPCASE\n//IN DD DSN=IN.NAMES,DISP=(OLD,DELETE)\n//OUT DD DSN=OUT.NAMES,DISP=(NEW,CATLG)\n";
    let expect = |want: &str, extra: &[&str]| {
        fs::write(dir.join("prod/OUT.NAMES"), want).unwrap();
        let mut args = vec!["--expected".to_string(), format!("DATASETS={}", dir.join("prod").display()), "--statement".into(), dir.join("st.json").display().to_string()];
        args.extend(extra.iter().map(|s| s.to_string()));
        let o = job_with(&dir, jcl, &args.iter().map(String::as_str).collect::<Vec<_>>());
        (o.status.code(), fs::read_to_string(dir.join("st.json")).unwrap_or_default(), log(&o))
    };
    let (code, st, l) = expect("ALPHA\nBETA\n", &[]);
    assert_eq!(code, Some(0), "{l}");
    assert!(st.contains("\"verdict\":\"equivalent\"") && st.contains("job-equivalence-v1") && st.contains("\"name\":\"program:UPCASE.cbl\"") && st.contains("\"clock\":\"2026-01-01T00:00:00.000Z\""), "{st}");
    assert!(dir.join("data/IN.NAMES").exists() && !dir.join("data/OUT.NAMES").exists(), "production's data sets are not touched");
    let (code, st, _) = expect("ALPHA\nBETX\n", &[]);
    assert_eq!(code, Some(1));
    assert!(st.contains("\"firstDifference\":{\"line\":2,\"offset\":9}"), "{st}");
    fs::write(dir.join("declare.txt"), "DATASET OUT.NAMES lines 2-2 the second name is spelt as production spelt it\n").unwrap();
    let (code, st, _) = expect("ALPHA\nBETX\n", &["--declare", dir.join("declare.txt").to_str().unwrap()]);
    assert_eq!(code, Some(0));
    assert!(st.contains("equivalent-as-declared"), "{st}");
    fs::write(dir.join("steps.txt"), "UP RC=0000\n").unwrap();
    let steps = format!("STEPS={}", dir.join("steps.txt").display());
    let (code, _, l) = expect("ALPHA\nBETA\n", &["--expected", &steps]);
    assert_eq!(code, Some(0), "{l}");
    fs::write(dir.join("steps.txt"), "UP RC=0004\n").unwrap();
    let (code, st, _) = expect("ALPHA\nBETA\n", &["--expected", &steps]);
    assert_eq!(code, Some(1));
    assert!(st.contains("\"what\":\"STEP UP\"") && st.contains("\"actual\":\"RC=0000\""), "{st}");
}

#[test]
fn generations_roll_forward_and_off_across_runs() {
    let dir = temp("gdg");
    let define = "//DEF EXEC PGM=IDCAMS\n//SYSPRINT DD DUMMY\n//SYSIN DD *\n  DEFINE GDG(NAME(PAY.DAILY) LIMIT(2) SCRATCH)\n/*\n";
    let o = job(&dir, define);
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    assert!(fs::read_to_string(dir.join("data/PAY.DAILY")).unwrap().starts_with("IRONWORK-GDG LIMIT=2 SCRATCH"));
    let run = |text: &str| {
        let jcl = format!("//NEW EXEC PGM=IEBGENER\n//SYSUT1 DD *\n{text}\n/*\n//SYSUT2 DD DSN=PAY.DAILY(+1),DISP=(NEW,CATLG)\n//SYSIN DD DUMMY\n//SAME EXEC PGM=IEBGENER\n//SYSUT1 DD DSN=*.NEW.SYSUT2,DISP=SHR\n//SYSUT2 DD SYSOUT=*\n//SYSIN DD DUMMY\n");
        job(&dir, &jcl)
    };
    for day in ["one", "two", "three"] {
        let o = run(day);
        assert_eq!(o.status.code(), Some(0), "{}", log(&o));
        assert_eq!(String::from_utf8_lossy(&o.stdout), format!("{day}\n"));
    }
    assert!(!dir.join("data/PAY.DAILY.G0001V00").exists());
    assert_eq!(fs::read_to_string(dir.join("data/PAY.DAILY.G0002V00")).unwrap(), "two\n");
    assert_eq!(fs::read_to_string(dir.join("data/PAY.DAILY.G0003V00")).unwrap(), "three\n");
    let o = job(&dir, "//ALL EXEC PGM=IEBGENER\n//SYSUT1 DD DSN=PAY.DAILY,DISP=SHR\n//SYSUT2 DD SYSOUT=*\n//SYSIN DD DUMMY\n//PREV EXEC PGM=IEBGENER\n//SYSUT1 DD DSN=PAY.DAILY(-1),DISP=SHR\n//SYSUT2 DD SYSOUT=*\n//SYSIN DD DUMMY\n");
    assert_eq!(String::from_utf8_lossy(&o.stdout), "three\ntwo\ntwo\n");
    let o = job(&dir, "//BAD EXEC PGM=IEFBR14\n//X DD DSN=PAY.DAILY(-5),DISP=SHR\n");
    assert!(log(&o).contains("DD X: PAY.DAILY(-5): no such generation"), "{}", log(&o));
}

#[test]
fn a_job_records_its_steps_in_a_hash_chained_journal() {
    let dir = temp("evidence");
    upcase(&dir);
    fs::write(dir.join("data/IN.NAMES"), "alpha\n").unwrap();
    let ev = dir.with_extension("evidence");
    let _ = fs::remove_dir_all(&ev);
    let o = job_with(
        &dir,
        "//UP EXEC PGM=UPCASE\n//IN DD DSN=IN.NAMES,DISP=SHR\n//OUT DD DSN=OUT.NAMES,DISP=(NEW,CATLG)\n//SKIP EXEC PGM=IEFBR14,COND=(0,EQ)\n//GONE EXEC PGM=IEFBR14\n//OLD DD DSN=IN.NAMES,DISP=(OLD,DELETE)\n",
        &["--evidence", ev.to_str().unwrap()],
    );
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    let runs: Vec<_> = fs::read_dir(ev.join("runs")).unwrap().flatten().collect();
    assert_eq!(runs.len(), 1);
    let text = fs::read_to_string(runs[0].path()).unwrap();
    let kinds: Vec<&str> = text.lines().map(|l| l.split("\"kind\":\"").nth(1).unwrap().split('"').next().unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"open"));
    assert_eq!(kinds.last(), Some(&"close"));
    assert_eq!(kinds.iter().filter(|k| **k == "step").count(), 3);
    for want in ["\"outcome\":\"RC=0000\",\"pgm\":\"UPCASE\"", "\"outcome\":\"BYPASSED: COND=(0,EQ) is true\"", "\"path\":\"job.jcl\"", "\"path\":\"lib/UPCASE.cbl\"", "\"dd\":\"OUT\",\"event\":\"end\"", "\"dd\":\"OLD\",\"event\":\"end\""] {
        assert!(text.contains(want), "{want} in {text}");
    }
    assert!(!text.contains("alpha") && !text.contains("ALPHA"));
    assert!(fs::read_to_string(ev.join("ledger.jsonl")).unwrap().contains("\"kind\":\"run\""));
}

#[test]
fn disp_mod_writes_after_what_the_data_set_holds_and_creates_one_that_is_missing() {
    let dir = temp("mod");
    upcase(&dir);
    fs::write(dir.join("data/IN.NAMES"), "delta\n").unwrap();
    fs::write(dir.join("data/LOG.NAMES"), "ALPHA\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//UP EXEC PGM=UPCASE\n//IN DD DSN=IN.NAMES,DISP=SHR\n//OUT DD DSN=LOG.NAMES,DISP=MOD\n",
            "//GEN EXEC PGM=IEBGENER\n//SYSUT1 DD *\nlast\n/*\n//SYSUT2 DD DSN=LOG.NAMES,DISP=MOD\n//SYSIN DD DUMMY\n",
            "//NEW EXEC PGM=IEBGENER\n//SYSUT1 DD *\nfirst\n/*\n//SYSUT2 DD DSN=MADE.BY.MOD,DISP=(MOD,CATLG)\n//SYSIN DD DUMMY\n",
            "//GONE EXEC PGM=IEBGENER\n//SYSUT1 DD *\nx\n/*\n//SYSUT2 DD DSN=NOT.KEPT,DISP=MOD\n//SYSIN DD DUMMY\n",
        ),
    );
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    assert_eq!(fs::read_to_string(dir.join("data/LOG.NAMES")).unwrap(), "ALPHA\nDELTA\nlast\n");
    assert_eq!(fs::read_to_string(dir.join("data/MADE.BY.MOD")).unwrap(), "first\n");
    assert!(!dir.join("data/NOT.KEPT").exists(), "a data set DISP=MOD created with no disposition is deleted as NEW would be");
}

#[test]
fn parm_reaches_the_main_program_as_language_environment_passes_it() {
    let dir = temp("parm");
    let text = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. SHOWPARM.",
        "DATA DIVISION.",
        "LINKAGE SECTION.",
        "01 PARM-AREA.",
        "    05 PARM-LEN PIC S9(4) COMP.",
        "    05 PARM-TEXT PIC X(100).",
        "PROCEDURE DIVISION USING PARM-AREA.",
        "    IF PARM-LEN > 0",
        "        DISPLAY PARM-LEN ' ' PARM-TEXT(1:PARM-LEN)",
        "    ELSE",
        "        DISPLAY PARM-LEN",
        "    END-IF.",
        "    IF PARM-TEXT = LOW-VALUES",
        "        DISPLAY 'ZEROS PAST THE ARGUMENTS'",
        "    END-IF.",
        "    GOBACK.",
    ]);
    fs::write(dir.join("lib/SHOWPARM.cbl"), text).unwrap();
    let o = job(&dir, "//P1 EXEC PGM=SHOWPARM,PARM='RUN=1,MODE=X/RPTOPTS(ON)'\n//P2 EXEC PGM=SHOWPARM,PARM='11/16/1967'\n//P3 EXEC PGM=SHOWPARM\n");
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "0012 RUN=1,MODE=X\n0010 11/16/1967\n0000\nZEROS PAST THE ARGUMENTS\n");
}

#[test]
fn load_libraries_work_files_implied_sysin_and_mod_on_generations_and_concatenations() {
    let dir = temp("system");
    upcase(&dir);
    fs::write(dir.join("data/IN.NAMES"), "gamma\n").unwrap();
    fs::write(dir.join("data/PART.TWO"), "two\n").unwrap();
    let o = job(&dir, "//DEF EXEC PGM=IDCAMS\n//SYSPRINT DD DUMMY\n//SYSIN DD *\n  DEFINE GDG(NAME(RUN.LOG) LIMIT(3))\n/*\n");
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    let o = job(
        &dir,
        concat!(
            "//JOBLIB   DD DSN=PROD.LOADLIB,DISP=SHR\n",
            "//         DD DSN=COMMON.LOADLIB,DISP=SHR\n",
            "//WORK     EXEC PGM=IEBGENER\n",
            "//SYSUT1   DD DSN=IN.NAMES,DISP=SHR\n",
            "//SYSUT2   DD UNIT=SYSDA,SPACE=(CYL,(1,1)),DISP=(NEW,PASS)\n",
            "//SYSIN    DD DUMMY\n",
            "//UP       EXEC PGM=UPCASE\n",
            "//STEPLIB  DD DSN=APP.LOADLIB,DISP=SHR\n",
            "//IN       DD DSN=*.WORK.SYSUT2,DISP=(OLD,DELETE)\n",
            "//         DD DSN=PART.ONE,DISP=MOD\n",
            "//         DD DSN=PART.TWO,DISP=MOD\n",
            "//OUT      DD DSN=RUN.LOG(+1),DISP=(MOD,CATLG)\n",
            "//SORTWK01 DD UNIT=SYSDA,SPACE=(CYL,(10,10))\n",
            "0004\n",
        ),
    );
    assert_eq!(o.status.code(), Some(4), "{}", log(&o));
    assert_eq!(fs::read_to_string(dir.join("data/RUN.LOG.G0001V00")).unwrap(), "GAMMA\nTWO\n");
    assert!(!dir.join("data/PART.ONE").exists(), "DISP=MOD made PART.ONE for the step and deleted it as NEW would");
    let o = job(&dir, "//MORE EXEC PGM=IEBGENER\n//SYSUT1 DD *\nlater\n/*\n//SYSUT2 DD DSN=RUN.LOG(0),DISP=MOD\n//SYSIN DD DUMMY\n");
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    assert_eq!(fs::read_to_string(dir.join("data/RUN.LOG.G0001V00")).unwrap(), "GAMMA\nTWO\nlater\n");
}

#[test]
fn sort_orders_records_as_dfsort_does_and_merges_sorted_inputs() {
    let dir = temp("sort");
    fs::write(dir.join("data/IN.KEYS"), "B 003\nA 001\nC 002\nA 001\nb 009\n1 000\n").unwrap();
    fs::write(dir.join("data/M.ONE"), "A\nC\n").unwrap();
    fs::write(dir.join("data/M.TWO"), "B\nD\n").unwrap();
    fs::write(dir.join("data/M.BAD"), "D\nB\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//S1 EXEC PGM=SORT\n//SYSOUT DD SYSOUT=*\n//SORTIN DD DSN=IN.KEYS,DISP=SHR\n//SORTOUT DD DSN=OUT.KEYS,DISP=(NEW,CATLG)\n",
            "//SYSIN DD *\n  SORT FIELDS=(1,1,CH,A,3,3,ZD,A)\n  SUM FIELDS=NONE\n/*\n",
            "//S2 EXEC PGM=ICEMAN\n//SYSOUT DD SYSOUT=*\n//SORTIN01 DD DSN=M.ONE,DISP=SHR\n//SORTIN02 DD DSN=M.TWO,DISP=SHR\n//SORTOUT DD DSN=M.OUT,DISP=(NEW,CATLG)\n",
            "//SYSIN DD *\n  MERGE FIELDS=(1,1,CH,A)\n/*\n",
            "//S3 EXEC PGM=SORT\n//SYSOUT DD SYSOUT=*\n//SORTIN01 DD DSN=M.BAD,DISP=SHR\n//SORTOUT DD DUMMY\n//SYSIN DD *\n  MERGE FIELDS=(1,1,CH,A)\n/*\n",
        ),
    );
    let l = log(&o);
    assert!(l.contains("S1 PGM=SORT RC=0000") && l.contains("S2 PGM=ICEMAN RC=0000") && l.contains("S3 PGM=SORT RC=0016"), "{l}");
    assert_eq!(fs::read_to_string(dir.join("data/OUT.KEYS")).unwrap(), "b 009\nA 001\nB 003\nC 002\n1 000\n", "lower case before upper before digits, as EBCDIC orders them");
    assert_eq!(fs::read_to_string(dir.join("data/M.OUT")).unwrap(), "A\nB\nC\nD\n");
    assert!(String::from_utf8_lossy(&o.stdout).contains("record 2 of DD SORTIN01 is out of order for the MERGE"));
}

#[test]
fn sort_reads_fixed_records_by_their_length_and_refuses_what_it_does_not_model() {
    let dir = temp("sortbin");
    fs::write(dir.join("data/BIN.IN"), [0u8, 0, 0, 2, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0xff]).unwrap();
    let path = dir.join("job.jcl");
    fs::write(&path, "//T JOB 1\n//S1 EXEC PGM=SORT\n//SORTIN DD DSN=BIN.IN,DISP=SHR,DCB=(RECFM=FB,LRECL=4)\n//SORTOUT DD DSN=BIN.OUT,DISP=(NEW,CATLG)\n//SYSIN DD *\n  SORT FIELDS=(1,4,FI,A)\n/*\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).args(["job", path.to_str().unwrap(), "--datasets", dir.join("data").to_str().unwrap()]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", log(&o));
    assert_eq!(fs::read(dir.join("data/BIN.OUT")).unwrap(), [0xff, 0xff, 0xff, 0xff, 0, 0, 0, 1, 0, 0, 0, 2], "FI is signed: -1 sorts first");
    let o = job(&dir, "//S1 EXEC PGM=SORT\n//SYSIN DD *\n  SORT FIELDS=(1,1,CH,A)\n  OUTREC IFTHEN=(WHEN=(1,1,CH,EQ,C'A'),OVERLAY=(2:C'B'))\n/*\n");
    assert_eq!(o.status.code(), Some(2));
    assert!(log(&o).contains("SORT: the OUTREC parameter IFTHEN is not supported yet"), "{}", log(&o));
}

#[test]
fn sort_selects_reformats_and_splits_records_as_dfsort_does() {
    let dir = temp("sortedit");
    fs::write(dir.join("data/IN.EMP"), "A001 SMITH  0100\nB002 JONES  0200\nA003 BROWN  0050\nC004 GREEN  0300\n").unwrap();
    let o = job(
        &dir,
        concat!(
            "//S1 EXEC PGM=SORT\n//SYSOUT DD SYSOUT=*\n//SORTIN DD DSN=IN.EMP,DISP=SHR\n",
            "//BIG DD DSN=OUT.BIG,DISP=(NEW,CATLG)\n//REST DD DSN=OUT.REST,DISP=(NEW,CATLG)\n//ALLB DD DSN=OUT.ALLB,DISP=(NEW,CATLG)\n",
            "//SYSIN DD *\n",
            "  OMIT COND=(1,1,CH,EQ,C'C')\n",
            "  INREC BUILD=(1,4,C'-',6,6,13,4)\n",
            "  SORT FIELDS=(12,4,ZD,D)\n",
            "  OUTREC OVERLAY=(16:C'*')\n",
            "  OUTFIL FNAMES=BIG,INCLUDE=(12,4,ZD,GE,100),BUILD=(1,4,X,12,4)\n",
            "  OUTFIL FNAMES=REST,SAVE\n",
            "  OUTFIL FNAMES=ALLB,INCLUDE=(1,1,CH,EQ,C'B')\n",
            "/*\n",
        ),
    );
    assert_eq!(o.status.code(), Some(0), "{}{}", log(&o), String::from_utf8_lossy(&o.stdout));
    assert_eq!(fs::read_to_string(dir.join("data/OUT.BIG")).unwrap(), "B002 0200\nA001 0100\n");
    assert_eq!(fs::read_to_string(dir.join("data/OUT.REST")).unwrap(), "A003-BROWN 0050*\n", "SAVE keeps what no other group selects");
    assert_eq!(fs::read_to_string(dir.join("data/OUT.ALLB")).unwrap(), "B002-JONES 0200*\n");
    assert!(String::from_utf8_lossy(&o.stdout).contains("ironwork SORT: 2 records written to BIG"));
}
