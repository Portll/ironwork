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
    let o = job(&dir, "//S1 EXEC PGM=IEFBR14\n//NEW DD DSN=MADE.EARLY,DISP=(NEW,CATLG)\n//S2 EXEC PGM=SORT\n//S3 EXEC PGM=IEFBR14\n//X DD DSN=A.B,DISP=MOD\n");
    assert_eq!(o.status.code(), Some(2));
    let l = log(&o);
    assert!(l.contains("PGM=SORT is not supported yet") && l.contains("DISP=MOD is not supported yet"), "{l}");
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
