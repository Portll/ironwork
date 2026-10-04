//! `ironwork fuzz --interface`: a subprogram that adds a quantity from its first argument ends in a
//! data exception on generated arguments; where a program CALLs it, each run takes that CALL's
//! shape, and its second argument stays OMITTED.

mod schema;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use schema::read_manifest;

const SUBPROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. ADDQTY.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "       01  WS-TOTAL PIC 9(9) VALUE 0.",
    "       LINKAGE SECTION.",
    "       01  QTY-REC.",
    "           05 QTY PIC 9(5).",
    "       01  NOTE-REC PIC X(8).",
    "       PROCEDURE DIVISION USING QTY-REC NOTE-REC.",
    "           IF ADDRESS OF NOTE-REC NOT = NULL",
    "              DISPLAY NOTE-REC",
    "           END-IF",
    "           ADD QTY TO WS-TOTAL",
    "           GOBACK.",
];

const CALLER: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. MAINP.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "       01  WS-QTY PIC 9(5) VALUE 1.",
    "       PROCEDURE DIVISION.",
    "           CALL 'ADDQTY' USING WS-QTY OMITTED",
    "           GOBACK.",
];

fn repo(name: &str, caller: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-fuzz-iface-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("repo/src")).unwrap();
    fs::write(dir.join("repo/src/ADDQTY.cbl"), SUBPROGRAM.join("\n") + "\n").unwrap();
    if caller {
        fs::write(dir.join("repo/src/MAINP.cbl"), CALLER.join("\n") + "\n").unwrap();
    }
    dir
}

fn ironwork(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(args).output().unwrap()
}

fn fuzz(dir: &Path, program: &str) -> Output {
    let out = dir.join("run");
    ironwork(dir, &["fuzz", "--interface", program, "-L", "src", "--runs", "40", "-o", out.to_str().unwrap()])
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_subprogram_s_abend_is_kept_with_its_arguments_shaped_by_the_call_that_passes_them() {
    let dir = repo("caller", true);
    let o = fuzz(&dir, "src/ADDQTY.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"format\":\"ironwork-fuzz-interface/v1\""), "{manifest}");
    assert!(manifest.contains("\"entry\":\"interface\""), "{manifest}");
    assert!(manifest.contains("\"callers\":[{\"file\":\"src/MAINP.cbl\",\"line\":7}]"), "{manifest}");
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"ADDQTY.cbl\",\"line\":14"), "{manifest}");
    assert!(manifest.contains("\"name\":\"NOTE-REC\",\"omitted\":true,\"position\":1"), "{manifest}");
    let journal = manifest.split("\"journal\":\"").nth(1).and_then(|r| r.split('"').next()).expect("a kept run's journal");
    let text = fs::read_to_string(dir.join("run/evidence/runs").join(format!("{journal}.jsonl"))).unwrap();
    assert!(text.lines().any(|l| l.contains("\"kind\":\"abend\"") && l.contains("\"line\":14")), "{text}");
    // Every run's coverage is added up, the abending statement among those started.
    assert!(manifest.contains("\"runCoverage\":\"coverage/runs.json\""), "{manifest}");
    let covered = fs::read_to_string(dir.join("run/coverage/runs.json")).unwrap();
    assert!(covered.contains("{\"file\":\"ADDQTY.cbl\",\"line\":14,\"runs\":"), "{covered}");
}

#[test]
fn a_subprogram_no_program_calls_gets_every_argument_and_no_call_sites() {
    let dir = repo("alone", false);
    let o = fuzz(&dir, "src/ADDQTY.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"callers\":[]"), "{manifest}");
    assert!(!manifest.contains("\"omitted\":true"), "{manifest}");
}

#[test]
fn run_passes_each_argument_file_as_a_caller_would_and_omitted_as_a_null_address() {
    let dir = repo("run", false);
    let qty = dir.join("qty");
    fs::write(&qty, [0xF0, 0xF0, 0xF0, 0xF4, 0xF2]).unwrap();
    let o = ironwork(&dir, &["run", "src/ADDQTY.cbl", "--argument", qty.to_str().unwrap(), "--argument", "OMITTED"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let note = dir.join("note");
    fs::write(&note, [0xD5, 0xD6, 0xE3, 0xC5, 0x40, 0x40, 0x40, 0x40]).unwrap();
    let o = ironwork(&dir, &["run", "src/ADDQTY.cbl", "--argument", qty.to_str().unwrap(), "--argument", note.to_str().unwrap()]);
    assert_eq!(String::from_utf8_lossy(&o.stdout), "NOTE    \n", "{}", stderr(&o));
    let o = ironwork(&dir, &["run", "src/ADDQTY.cbl", "--argument", "OMITTED", "--parm", "X"]);
    assert!(!o.status.success() && stderr(&o).contains("--argument is for run, and not with --parm"), "{}", stderr(&o));
}

#[test]
fn an_ims_program_and_a_main_program_are_refused() {
    let dir = repo("refused", false);
    let ims = SUBPROGRAM.join("\n").replace("           ADD QTY TO WS-TOTAL", "           CALL 'CBLTDLI' USING QTY-REC");
    fs::write(dir.join("repo/src/IMSPGM.cbl"), ims + "\n").unwrap();
    let o = fuzz(&dir, "src/IMSPGM.cbl");
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("IMS"), "{}", stderr(&o));
    fs::write(dir.join("repo/src/MAINP.cbl"), CALLER.join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "src/MAINP.cbl");
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("fuzz it as a main program"), "{}", stderr(&o));
}

const FILE_SUBPROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. FILESUB.",
    "       ENVIRONMENT DIVISION.",
    "       INPUT-OUTPUT SECTION.",
    "       FILE-CONTROL.",
    "           SELECT IN-FILE ASSIGN TO INDD.",
    "           SELECT OUT-FILE ASSIGN TO OUTDD.",
    "       DATA DIVISION.",
    "       FILE SECTION.",
    "       FD  IN-FILE.",
    "       01  IN-REC PIC X(10).",
    "       FD  OUT-FILE.",
    "       01  OUT-REC PIC X(10).",
    "       LINKAGE SECTION.",
    "       01  QTY-REC.",
    "           05 QTY PIC 9(5).",
    "       PROCEDURE DIVISION USING QTY-REC.",
    "           OPEN INPUT IN-FILE OUTPUT OUT-FILE",
    "           READ IN-FILE AT END CONTINUE END-READ",
    "           WRITE OUT-REC FROM QTY-REC",
    "           CLOSE IN-FILE OUT-FILE",
    "           ADD 1 TO QTY",
    "           GOBACK.",
];

/// The subprogram's files get data sets as the main fuzz gives those it does not vary, so its
/// OPEN succeeds and the runs reach the abend the arguments cause.
#[test]
fn a_subprogram_s_files_are_given_data_sets_so_its_runs_get_past_open() {
    let dir = repo("files", false);
    fs::write(dir.join("repo/src/FILESUB.cbl"), FILE_SUBPROGRAM.join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "src/FILESUB.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("ironwork fuzz: not varied, given empty: INDD"), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"FILESUB.cbl\",\"line\":22"), "{manifest}");
    assert!(!manifest.contains("IO-35"), "{manifest}");
}
