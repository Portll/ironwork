//! IDCAMS in `ironwork job`: alternate indexes built over a cluster, paths read through them,
//! LISTCAT over the catalog and PRINT of a data set's records.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-idcams-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

fn job(dir: &Path, jcl: &str) -> Output {
    let path = dir.join("job.jcl");
    fs::write(&path, format!("//TESTJOB JOB (1),'T',CLASS=A\n{jcl}")).unwrap();
    Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .args(["job", path.to_str().unwrap(), "--datasets"])
        .arg(format!("{}:text", dir.join("data").display()))
        .args(["-L", dir.join("lib").to_str().unwrap(), "--clock", "2026-01-01T00:00:00"])
        .output()
        .unwrap()
}

fn log(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A program on `dd`, an indexed file of 20-byte employee records keyed by `key`: an id in 1-4
/// and a surname in 5-10, with the statements `body` after the file's description.
fn program(dir: &Path, id: &str, dd: &str, key: &str, body: &[&str]) {
    let head = [
        "IDENTIFICATION DIVISION.".to_string(),
        format!("PROGRAM-ID. {id}."),
        "ENVIRONMENT DIVISION.".into(),
        "INPUT-OUTPUT SECTION.".into(),
        "FILE-CONTROL.".into(),
        format!("    SELECT E-FILE ASSIGN TO {dd} ORGANIZATION INDEXED"),
        format!("        ACCESS DYNAMIC RECORD KEY {key} FILE STATUS FS."),
        "DATA DIVISION.".into(),
        "FILE SECTION.".into(),
        "FD E-FILE.".into(),
        "01 E-REC.".into(),
        "    05 E-ID PIC X(4).".into(),
        "    05 E-NAME PIC X(6).".into(),
        "    05 E-REST PIC X(10).".into(),
        "WORKING-STORAGE SECTION.".into(),
        "01 FS PIC XX.".into(),
        "PROCEDURE DIVISION.".into(),
    ];
    let text: String = head.iter().map(String::as_str).chain(body.iter().copied()).map(|l| format!("       {l}\n")).collect();
    fs::write(dir.join(format!("lib/{id}.cbl")), text).unwrap();
}

/// READP lists the file in key order; ADDK adds a record.
fn programs(dir: &Path) {
    let list = ["    OPEN INPUT E-FILE.", "    PERFORM UNTIL FS NOT = '00'", "        READ E-FILE NEXT", "        IF FS = '00' DISPLAY E-REC END-IF", "    END-PERFORM.", "    CLOSE E-FILE.", "    GOBACK."];
    program(dir, "READP", "PATHDD", "E-NAME", &list);
    program(dir, "ADDK", "KSDS", "E-ID", &["    OPEN I-O E-FILE.", "    MOVE '0004BAKER Delta' TO E-REC.", "    WRITE E-REC.", "    CLOSE E-FILE.", "    GOBACK."]);
}

const DEFINE_AND_BUILD: &str = concat!(
    "//DEF      EXEC PGM=IDCAMS\n",
    "//SYSPRINT DD SYSOUT=*\n",
    "//IN       DD *\n",
    "0001SMITH Alpha\n",
    "0002JONES Beta\n",
    "0003ADAMS Gamma\n",
    "/*\n",
    "//SYSIN    DD *\n",
    "  DEFINE CLUSTER(NAME(EMP.KSDS) INDEXED KEYS(4 0) -\n",
    "         RECORDSIZE(20 20))\n",
    "  REPRO INFILE(IN) OUTDATASET(EMP.KSDS)\n",
    "  DEFINE AIX(NAME(EMP.AIX) RELATE(EMP.KSDS) KEYS(6 4) UNIQUEKEY)\n",
    "  BLDINDEX INDATASET(EMP.KSDS) OUTDATASET(EMP.AIX)\n",
    "  DEFINE PATH(NAME(EMP.PATH) PATHENTRY(EMP.AIX))\n",
    "/*\n",
);

#[test]
fn a_program_reads_a_cluster_through_a_path_in_alternate_key_order_and_sees_what_the_upgrade_set_keeps() {
    let dir = temp("path");
    programs(&dir);
    let o = job(
        &dir,
        &format!("{DEFINE_AND_BUILD}//READ1 EXEC PGM=READP\n//PATHDD DD DSN=EMP.PATH,DISP=SHR\n//ADD EXEC PGM=ADDK\n//KSDS DD DSN=EMP.KSDS,DISP=OLD\n//READ2 EXEC PGM=READP\n//PATHDD DD DSN=EMP.PATH,DISP=SHR\n"),
    );
    assert_eq!(o.status.code(), Some(0), "{}{}", log(&o), stdout(&o));
    let out = stdout(&o);
    assert!(out.contains("IDC0652I EMP.AIX SUCCESSFULLY BUILT"), "{out}");
    let shown: Vec<&str> = out.lines().filter(|l| l.starts_with("000")).map(str::trim_end).collect();
    assert_eq!(shown, ["0003ADAMS Gamma", "0002JONES Beta", "0001SMITH Alpha", "0003ADAMS Gamma", "0004BAKER Delta", "0002JONES Beta", "0001SMITH Alpha"]);
    assert_eq!(fs::read_to_string(dir.join("data/EMP.KSDS")).unwrap(), "0001SMITH Alpha\n0002JONES Beta\n0003ADAMS Gamma\n0004BAKER Delta\n");
}

#[test]
fn a_noupgrade_index_stays_as_bldindex_left_it_until_it_is_built_again() {
    let dir = temp("noupgrade");
    programs(&dir);
    let jcl = DEFINE_AND_BUILD.replace("KEYS(6 4) UNIQUEKEY)", "KEYS(6 4) UNIQUEKEY NUPG)");
    let rebuild = "//BIX EXEC PGM=IDCAMS\n//SYSPRINT DD SYSOUT=*\n//SYSIN DD *\n  BLDINDEX INDATASET(EMP.KSDS) OUTDATASET(EMP.AIX)\n/*\n";
    let o = job(&dir, &format!("{jcl}//ADD EXEC PGM=ADDK\n//KSDS DD DSN=EMP.KSDS,DISP=OLD\n//READ1 EXEC PGM=READP\n//PATHDD DD DSN=EMP.PATH,DISP=SHR\n{rebuild}//READ2 EXEC PGM=READP\n//PATHDD DD DSN=EMP.PATH,DISP=SHR\n"));
    assert_eq!(o.status.code(), Some(0), "{}{}", log(&o), stdout(&o));
    let shown: Vec<String> = stdout(&o).lines().filter(|l| l.starts_with("000")).map(|l| l[..4].to_string()).collect();
    assert_eq!(shown, ["0003", "0002", "0001", "0003", "0004", "0002", "0001"]);
}

#[test]
fn bldindex_keeps_the_first_prime_key_of_a_duplicate_unique_key_and_deleting_the_cluster_takes_its_index_and_path() {
    let dir = temp("dupes");
    let jcl = DEFINE_AND_BUILD.replace("0002JONES Beta", "0002SMITH Beta");
    let o = job(&dir, &format!("{jcl}//DEL EXEC PGM=IDCAMS\n//SYSPRINT DD SYSOUT=*\n//SYSIN DD *\n  DELETE EMP.KSDS CLUSTER\n/*\n"));
    let out = stdout(&o);
    assert!(log(&o).contains("DEF PGM=IDCAMS RC=0008"), "{}", log(&o));
    assert!(out.contains("IDC1645I") && out.contains("IDC1652I EMP.AIX BUILT WITH ERRORS"), "{out}");
    for want in ["IDC0550I ENTRY (R) EMP.PATH DELETED", "IDC0550I ENTRY (G) EMP.AIX DELETED", "IDC0550I ENTRY (C) EMP.KSDS DELETED"] {
        assert!(out.contains(want), "{want} in {out}");
    }
    let left: Vec<String> = fs::read_dir(dir.join("data")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(left.is_empty(), "{left:?}");
}
