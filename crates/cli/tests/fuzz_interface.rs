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

const LOGGING_SUBPROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. LOGSUB.",
    "       DATA DIVISION.",
    "       LINKAGE SECTION.",
    "       01  QTY-REC.",
    "           05 QTY PIC 9(5).",
    "       PROCEDURE DIVISION USING QTY-REC.",
    "           CALL 'LOGGER'",
    "           ADD 1 TO QTY",
    "           GOBACK.",
];

const LOGGER: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. LOGGER.",
    "       ENVIRONMENT DIVISION.",
    "       INPUT-OUTPUT SECTION.",
    "       FILE-CONTROL.",
    "           SELECT LOG-FILE ASSIGN TO LOGDD.",
    "           SELECT RPT-FILE ASSIGN TO RPTDD.",
    "       DATA DIVISION.",
    "       FILE SECTION.",
    "       FD  LOG-FILE.",
    "       01  LOG-REC PIC X(10).",
    "       FD  RPT-FILE.",
    "       01  RPT-REC PIC X(10).",
    "       PROCEDURE DIVISION.",
    "           OPEN EXTEND LOG-FILE OUTPUT RPT-FILE",
    "           WRITE LOG-REC FROM 'CALLED'",
    "           WRITE RPT-REC FROM 'CALLED'",
    "           CLOSE LOG-FILE RPT-FILE",
    "           GOBACK.",
];

/// A program the subprogram CALLs from a library gets data sets for its files too, and a file it
/// OPENs EXTEND gets one that exists, so the runs get past that program's OPEN.
#[test]
fn a_called_program_s_files_are_given_data_sets_and_an_extended_one_exists() {
    let dir = repo("called", false);
    fs::write(dir.join("repo/src/LOGSUB.cbl"), LOGGING_SUBPROGRAM.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/LOGGER.cbl"), LOGGER.join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "src/LOGSUB.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("ironwork fuzz: not varied, given empty: LOGDD"), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"LOGSUB.cbl\",\"line\":9"), "{manifest}");
    assert!(!manifest.contains("IO-35"), "{manifest}");
}

/// A source that holds only a user-defined function is refused before any run.
#[test]
fn a_user_defined_function_is_refused() {
    let dir = repo("function", false);
    let function = [
        "       IDENTIFICATION DIVISION.",
        "       FUNCTION-ID. TWICE.",
        "       DATA DIVISION.",
        "       LINKAGE SECTION.",
        "       01  N PIC 9(5).",
        "       01  R PIC 9(6).",
        "       PROCEDURE DIVISION USING N RETURNING R.",
        "           COMPUTE R = N * 2",
        "           GOBACK.",
        "       END FUNCTION TWICE.",
    ];
    fs::write(dir.join("repo/src/TWICE.cbl"), function.join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "src/TWICE.cbl");
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("TWICE is a user-defined function"), "{}", stderr(&o));
}

const NAMED_CALL_SUBPROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. NAMESUB.",
    "       DATA DIVISION.",
    "       LINKAGE SECTION.",
    "       01  REQ.",
    "           05 LOG-PROGRAM PIC X(8).",
    "           05 QTY PIC 9(5).",
    "       PROCEDURE DIVISION USING REQ.",
    "           CALL LOG-PROGRAM",
    "           ADD 1 TO QTY",
    "           GOBACK.",
];

const NAMING_CALLER: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. NAMEMAIN.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "       01  WS-REQ.",
    "           05 WS-LOG-PROGRAM PIC X(8).",
    "           05 WS-QTY PIC 9(5) VALUE 1.",
    "       01  WS-OTHER PIC X(8).",
    "       PROCEDURE DIVISION.",
    "           MOVE 'LOGGIT' TO WS-LOG-PROGRAM",
    "           MOVE 'OTHERP' TO WS-OTHER",
    "           CALL 'NAMESUB' USING WS-REQ",
    "           GOBACK.",
];

/// A CALL whose target is an argument's field is given a program name a caller stores in an item of
/// that field's name, so the runs get past the CALL to the abend the other field causes.
#[test]
fn a_call_target_an_argument_supplies_is_given_a_program_the_callers_name() {
    let dir = repo("named", false);
    fs::write(dir.join("repo/src/NAMESUB.cbl"), NAMED_CALL_SUBPROGRAM.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/NAMEMAIN.cbl"), NAMING_CALLER.join("\n") + "\n").unwrap();
    for (id, line) in [("LOGGIT", "           GOBACK."), ("OTHERP", "           STOP RUN.")] {
        let text = format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       PROCEDURE DIVISION.\n{line}\n");
        fs::write(dir.join(format!("repo/src/{id}.cbl")), text).unwrap();
    }
    let o = fuzz(&dir, "src/NAMESUB.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("a CALL takes its program name from REQ at offset 0; runs give it one of LOGGIT\n"), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains(" 0 refused"), "{}", String::from_utf8_lossy(&o.stdout));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"NAMESUB.cbl\",\"line\":10"), "{manifest}");
}

/// A called program whose USING item is longer than what a CALL passes is told before the runs,
/// from the subprogram's side and from its caller's.
#[test]
fn an_argument_shorter_than_the_called_program_describes_is_told_before_the_runs() {
    let dir = repo("short", false);
    let callee = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. DATAPROG.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01  OPERATION-TYPE PIC X(6).",
        "       LINKAGE SECTION.",
        "       01  PASSED-OPERATION PIC X(6).",
        "       PROCEDURE DIVISION USING PASSED-OPERATION.",
        "           MOVE PASSED-OPERATION TO OPERATION-TYPE",
        "           GOBACK.",
    ];
    let caller = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. OPSPROG.",
        "       DATA DIVISION.",
        "       LINKAGE SECTION.",
        "       01  CHOICE PIC X(6).",
        "       PROCEDURE DIVISION USING CHOICE.",
        "           CALL 'DATAPROG' USING 'READ'",
        "           GOBACK.",
    ];
    fs::write(dir.join("repo/src/DATAPROG.cbl"), callee.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/OPSPROG.cbl"), caller.join("\n") + "\n").unwrap();
    let told = "ironwork fuzz: src/OPSPROG.cbl:7 passes a literal of 4 bytes as DATAPROG's PASSED-OPERATION of 6: the called program reads past it\n";
    for program in ["src/DATAPROG.cbl", "src/OPSPROG.cbl"] {
        let o = fuzz(&dir, program);
        assert!(stderr(&o).contains(told), "{program}: {}", stderr(&o));
        let _ = fs::remove_dir_all(dir.join("run"));
    }
}

/// A field the subprogram compares with a literal now and then holds that literal, so the runs reach
/// the branch it guards, which random text would not.
#[test]
fn a_field_takes_the_value_the_program_compares_it_with() {
    let dir = repo("dictionary", false);
    let source = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. GUARDED.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01  TOTAL PIC 9(7) VALUE 0.",
        "       LINKAGE SECTION.",
        "       01  REQ.",
        "           05 OP PIC X(6).",
        "           05 QTY PIC 9(5).",
        "       PROCEDURE DIVISION USING REQ.",
        "           IF OP = 'READ'",
        "              ADD QTY TO TOTAL",
        "           END-IF",
        "           GOBACK.",
    ];
    fs::write(dir.join("repo/src/GUARDED.cbl"), source.join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "src/GUARDED.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"GUARDED.cbl\",\"line\":12"), "{manifest}");
}

/// Each run stops at --hang-limit statements, so a loop the arguments cause ends in S322 and is kept,
/// whatever the machine's load does to the clock.
#[test]
fn a_loop_the_arguments_cause_ends_at_the_statement_limit_and_is_kept() {
    let dir = repo("hang", false);
    let source = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. SPINNER.",
        "       DATA DIVISION.",
        "       LINKAGE SECTION.",
        "       01  REQ.",
        "           05 OP PIC X(6).",
        "       PROCEDURE DIVISION USING REQ.",
        "           PERFORM UNTIL OP NOT = 'LOOP'",
        "              CONTINUE",
        "           END-PERFORM",
        "           GOBACK.",
    ];
    fs::write(dir.join("repo/src/SPINNER.cbl"), source.join("\n") + "\n").unwrap();
    let out = dir.join("run");
    let o = ironwork(&dir, &["fuzz", "--interface", "src/SPINNER.cbl", "-L", "src", "--runs", "40", "--hang-limit", "20000", "--timeout", "60", "-o", out.to_str().unwrap()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&out);
    assert!(manifest.contains("\"code\":\"S322\",\"file\":\"SPINNER.cbl\",\"line\":9"), "{manifest}");
    assert!(String::from_utf8_lossy(&o.stdout).contains(" 0 timeout"), "{}", String::from_utf8_lossy(&o.stdout));
}

/// A CALL target that MOVEs carry from an argument, a group MOVE among them, is found before the
/// runs, so no run CALLs a generated name.
#[test]
fn a_call_target_moves_carry_from_an_argument_is_found_before_the_runs() {
    let dir = repo("chain", false);
    let source = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. CHAINSUB.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01  WS-COPY.",
        "           05 WS-COPY-PROGRAM PIC X(8).",
        "           05 WS-COPY-QTY PIC 9(5).",
        "       01  WS-PGM PIC X(8).",
        "       LINKAGE SECTION.",
        "       01  REQ.",
        "           05 LOG-PROGRAM PIC X(8).",
        "           05 QTY PIC 9(5).",
        "       PROCEDURE DIVISION USING REQ.",
        "           MOVE REQ TO WS-COPY",
        "           MOVE WS-COPY-PROGRAM TO WS-PGM",
        "           CALL WS-PGM",
        "           GOBACK.",
    ];
    let caller = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. CHAINMN.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01  WS-PGM PIC X(8).",
        "       PROCEDURE DIVISION.",
        "           MOVE 'LOGGIT' TO WS-PGM",
        "           CALL 'CHAINSUB' USING WS-PGM",
        "           GOBACK.",
    ];
    fs::write(dir.join("repo/src/CHAINSUB.cbl"), source.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/CHAINMN.cbl"), caller.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/LOGGIT.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LOGGIT.\n       PROCEDURE DIVISION.\n           GOBACK.\n").unwrap();
    let o = fuzz(&dir, "src/CHAINSUB.cbl");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("a CALL takes its program name from REQ at offset 0; runs give it one of LOGGIT\n"), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains(" 0 refused"), "{}", String::from_utf8_lossy(&o.stdout));
}

/// Runs that reach statements no earlier run did are kept and changed, so an abend four nested
/// comparisons deep, which a fresh draw reaches about once in 256 runs, is found within 120.
#[test]
fn inputs_that_reach_new_statements_are_changed_until_a_deep_branch_is_reached() {
    let dir = repo("guided", false);
    let source = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. NESTED.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01  TOTAL PIC 9(7) VALUE 0.",
        "       LINKAGE SECTION.",
        "       01  REQ.",
        "           05 K1 PIC XX.",
        "           05 K2 PIC XX.",
        "           05 K3 PIC XX.",
        "           05 K4 PIC XX.",
        "           05 QTY PIC 9(5).",
        "       PROCEDURE DIVISION USING REQ.",
        "           IF K1 = 'A1'",
        "              DISPLAY 'ONE'",
        "              IF K2 = 'B2'",
        "                 DISPLAY 'TWO'",
        "                 IF K3 = 'C3'",
        "                    DISPLAY 'THREE'",
        "                    IF K4 = 'D4'",
        "                       ADD QTY TO TOTAL",
        "                    END-IF",
        "                 END-IF",
        "              END-IF",
        "           END-IF",
        "           GOBACK.",
    ];
    fs::write(dir.join("repo/src/NESTED.cbl"), source.join("\n") + "\n").unwrap();
    let out = dir.join("run");
    let o = ironwork(&dir, &["fuzz", "--interface", "src/NESTED.cbl", "-L", "src", "--runs", "120", "-o", out.to_str().unwrap()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&out);
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"NESTED.cbl\",\"line\":21"), "{manifest}");
}
