//! IDCAMS PRINT in `ironwork job`: a key-sequenced, an entry-sequenced and a relative record
//! cluster, and a sequential data set, listed in DUMP, HEX and CHARACTER format over a range.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-idcams-print-{name}-{}", std::process::id()));
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

/// An IDCAMS step that defines a cluster and copies the records `lines` into it.
fn define(cluster: &str, lines: &[&str]) -> String {
    let name = cluster.split(['(', ')']).nth(2).unwrap();
    format!("//DEF      EXEC PGM=IDCAMS\n//SYSPRINT DD SYSOUT=*\n//IN       DD *\n{}\n/*\n//SYSIN    DD *\n  DEFINE {cluster}\n  REPRO INFILE(IN) OUTDATASET({name})\n/*\n", lines.join("\n"))
}

/// An IDCAMS step running the commands `sysin`, with the DDs `dds` beside SYSPRINT.
fn print_step(dds: &str, sysin: &str) -> String {
    format!("//PRT      EXEC PGM=IDCAMS\n//SYSPRINT DD SYSOUT=*\n{dds}//SYSIN    DD *\n{sysin}/*\n")
}

/// The lines of each PRINT's listing, from LISTING OF DATA SET to its IDC0001I or IDC3003I.
fn listings(out: &str) -> Vec<Vec<String>> {
    let mut all = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in out.lines().map(str::trim_end) {
        if line.starts_with("LISTING OF DATA SET") || (current.is_none() && line.starts_with("IDC3")) {
            current = Some(Vec::new());
        }
        if let Some(c) = current.as_mut() {
            c.push(line.to_string());
            if line.starts_with("IDC0001I") || line.starts_with("IDC3003I") {
                all.extend(current.take());
            }
        }
    }
    all
}

const KSDS: &str = "CLUSTER(NAME(PAY.KSDS) INDEXED KEYS(4 0) RECORDSIZE(20 20))";
const STAFF: [&str; 4] = ["AB01SMITH", "AB02Jones", "AC01ADAMS", "BA01BAKER"];

#[test]
fn a_key_sequenced_cluster_prints_in_dump_hex_and_character_by_count_skip_and_generic_keys() {
    let dir = temp("ksds");
    let sysin = concat!(
        "  PRINT INDATASET(PAY.KSDS) COUNT(1)\n",
        "  PRINT INFILE(KSDS) HEX SKIP(3)\n",
        "  PRINT INDATASET(PAY.KSDS) CHARACTER -\n",
        "        FROMKEY(AB0*) TOKEY(AC)\n",
        "  PRINT INFILE(KSDS) CHAR SKIP(1) COUNT(1)\n",
    );
    let o = job(&dir, &format!("{}{}", define(KSDS, &STAFF), print_step("//KSDS     DD DSN=PAY.KSDS,DISP=SHR\n", sysin)));
    assert!(log(&o).contains("PRT PGM=IDCAMS RC=0000"), "{}{}", log(&o), stdout(&o));
    let all = listings(&stdout(&o));
    let blanks = |n: usize| " ".repeat(n);
    assert_eq!(
        all[0],
        [
            "LISTING OF DATA SET -PAY.KSDS".to_string(),
            "KEY OF RECORD - C1C2F0F1".into(),
            format!("0000   C1C2F0F1 E2D4C9E3 C8404040 40404040  40404040{}*AB01SMITH{}*", blanks(30), blanks(23)),
            "IDC0005I NUMBER OF RECORDS PROCESSED WAS 1".into(),
            "IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS 0".into(),
        ]
    );
    assert_eq!(all[1][1..4], ["KEY OF RECORD - C2C1F0F1", &format!("C2C1F0F1C2C1D2C5D9{}", "40".repeat(11)), "IDC0005I NUMBER OF RECORDS PROCESSED WAS 1"]);
    assert_eq!(
        all[2][1..12],
        ["KEY OF RECORD - AB01", "", "AB01SMITH", "", "KEY OF RECORD - AB02", "", "AB02J....", "", "KEY OF RECORD - AC01", "", "AC01ADAMS"]
    );
    assert_eq!(all[2][12], "IDC0005I NUMBER OF RECORDS PROCESSED WAS 3");
    assert_eq!(all[3][1..4], ["KEY OF RECORD - AB02", "", "AB02J...."]);
}

#[test]
fn a_print_that_cannot_position_or_finds_the_cluster_empty_ends_with_code_12_and_the_next_command_runs() {
    let dir = temp("ends");
    let sysin = concat!(
        "  DEFINE CLUSTER(NAME(NONE.KSDS) INDEXED KEYS(4 0) RECORDSIZE(20 20))\n",
        "  PRINT INDATASET(NONE.KSDS)\n",
        "  PRINT INDATASET(PAY.KSDS) FROMKEY(C)\n",
        "  PRINT INDATASET(PAY.KSDS) TOKEY(AB012)\n",
        "  PRINT INDATASET(PAY.KSDS) TOKEY(AA)\n",
    );
    let o = job(&dir, &format!("{}{}", define(KSDS, &STAFF), print_step("", sysin)));
    let out = stdout(&o);
    assert!(log(&o).contains("PRT PGM=IDCAMS RC=0012"), "{}{out}", log(&o));
    for want in [
        "IDC3300I ERROR OPENING NONE.KSDS\nIDC3351I ** VSAM OPEN RETURN CODE IS 160\nIDC3003I FUNCTION TERMINATED. CONDITION CODE IS 12",
        "IDC3006I FUNCTION TERMINATED DUE TO BEGINNING POSITIONING ERROR\nIDC3003I FUNCTION TERMINATED. CONDITION CODE IS 12",
        "IDC3302I ACTION ERROR ON PAY.KSDS\nIDC3310I ** KEY SUPPLIED IS LONGER THAN KEY LENGTH OF DATA SET",
        "LISTING OF DATA SET -PAY.KSDS\nIDC0005I NUMBER OF RECORDS PROCESSED WAS 0\nIDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS 4",
    ] {
        assert!(out.contains(want), "{want}\nin\n{out}");
    }
}

#[test]
fn an_entry_sequenced_cluster_lists_byte_addresses_and_a_relative_record_cluster_record_numbers() {
    let dir = temp("esds-rrds");
    let esds = define("CLUSTER(NAME(LOG.ESDS) NONINDEXED RECORDSIZE(8 8))", &["FIRST", "SECOND", "THIRD"]);
    let rrds = define("CLUSTER(NAME(SLOT.RRDS) NUMBERED RECORDSIZE(6 6))", &["ONE", "TWO", "THREE"]).replace("//DEF ", "//DEF2");
    let sysin = concat!("  PRINT INDATASET(LOG.ESDS) CHARACTER SKIP(1)\n", "  PRINT INDATASET(SLOT.RRDS) HEX COUNT(2)\n", "  PRINT INDATASET(SLOT.RRDS) FROMKEY(ONE)\n");
    let o = job(&dir, &format!("{esds}{rrds}{}", print_step("", sysin)));
    let out = stdout(&o);
    assert!(log(&o).contains("PRT PGM=IDCAMS RC=0012"), "{}{out}", log(&o));
    let all = listings(&out);
    assert_eq!(all[0], ["LISTING OF DATA SET -LOG.ESDS", "RBA OF RECORD - 8", "", "SECOND", "", "RBA OF RECORD - 16", "", "THIRD", "IDC0005I NUMBER OF RECORDS PROCESSED WAS 2", "IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS 0"]);
    assert_eq!(all[1][1..6], ["RELATIVE RECORD NUMBER - 1", "D6D5C5404040", "", "RELATIVE RECORD NUMBER - 2", "E3E6D6404040"]);
    assert_eq!(all[2], ["IDC3302I ACTION ERROR ON SLOT.RRDS", "IDC3311I ** TYPE OF POSITIONING NOT SUPPORTED", "IDC3003I FUNCTION TERMINATED. CONDITION CODE IS 12"]);
}

#[test]
fn a_sequential_data_set_lists_record_sequence_numbers_by_name_or_through_its_dd() {
    let dir = temp("seq");
    fs::write(dir.join("data/CUST.LIST"), "ADAMS\nBAKER\nCLARK\nDAVIS\n").unwrap();
    fs::write(dir.join("data/EMPTY.LIST"), "").unwrap();
    let sysin = concat!("  PRINT INDATASET(CUST.LIST) CHARACTER SKIP(2) COUNT(1)\n", "  PRINT INFILE(SEQ) DUMP SKIP(3)\n", "  PRINT INDATASET(EMPTY.LIST) COUNT(1)\n");
    let o = job(&dir, &print_step("//SEQ      DD DSN=CUST.LIST,DISP=SHR\n", sysin));
    let out = stdout(&o);
    assert!(log(&o).contains("PRT PGM=IDCAMS RC=0004"), "{}{out}", log(&o));
    let all = listings(&out);
    assert_eq!(all[0], ["LISTING OF DATA SET -CUST.LIST", "RECORD SEQUENCE NUMBER - 3", "", "CLARK", "IDC0005I NUMBER OF RECORDS PROCESSED WAS 1", "IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS 0"]);
    assert_eq!(all[1][1..3], ["RECORD SEQUENCE NUMBER - 4".to_string(), format!("0000   C4C1E5C9 E2{}*DAVIS{}*", " ".repeat(64), " ".repeat(27))]);
    assert_eq!(all[2], ["LISTING OF DATA SET -EMPTY.LIST", "IDC0005I NUMBER OF RECORDS PROCESSED WAS 0", "IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS 4"]);
}
