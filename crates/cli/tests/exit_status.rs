//! The exit status of run, cics and job: a row of the reserved band, and of --exit-code's verdicts,
//! for each way a run ends.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A scratch directory with `src` for programs and `data` for a job's data sets, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ironwork-exit-status-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("data")).unwrap();
        Dir(dir)
    }

    /// Writes a program of these lines, each placed in Area B, as `src/NAME.cbl`.
    fn program(&self, name: &str, data: &[&str], procedure: &[&str]) -> String {
        let mut text = format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {name}.\n");
        if !data.is_empty() {
            text.push_str("       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n");
            data.iter().for_each(|l| text.push_str(&format!("       {l}\n")));
        }
        text.push_str("       PROCEDURE DIVISION.\n");
        procedure.iter().for_each(|l| text.push_str(&format!("           {l}\n")));
        let path = self.0.join(format!("src/{name}.cbl"));
        fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_owned()
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }

    /// Runs `job.jcl` holding a JOB statement and `steps`, its programs from `src`.
    fn job(&self, steps: &str, extra: &[&str]) -> Output {
        fs::write(self.0.join("job.jcl"), format!("//T JOB 1\n{steps}")).unwrap();
        ironwork(&[&["job", &self.path("job.jcl"), "--datasets", &self.path("data"), "-L", &self.path("src")], extra].concat())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// What `ironwork run` exits with, by default and with --exit-code, and what each said.
fn both(args: &[&str]) -> ((Option<i32>, String), (Option<i32>, String)) {
    let band = ironwork(args);
    let verdict = ironwork(&[args, &["--exit-code"]].concat());
    ((band.status.code(), stderr(&band)), (verdict.status.code(), stderr(&verdict)))
}

fn returning(dir: &Dir, rc: &str) -> String {
    dir.program("RC", &[], &[&format!("MOVE {rc} TO RETURN-CODE."), "GOBACK."])
}

const DATA_EXCEPTION: [&str; 3] = ["01 WS-A PIC X(3) VALUE '***'.", "01 WS-N REDEFINES WS-A PIC 9(3).", "01 WS-T PIC 9(3) VALUE 0."];

#[test]
fn a_return_code_from_0_to_238_is_the_exit_status_and_with_exit_code_0_is_0_and_any_other_1() {
    let dir = Dir::new("ended");
    for rc in [0, 4, 16, 238] {
        let ((band, said), (verdict, told)) = both(&["run", &returning(&dir, &rc.to_string())]);
        assert_eq!((band, said.as_str()), (Some(rc), ""), "{rc}");
        if rc == 0 {
            assert_eq!((verdict, told.as_str()), (Some(0), ""));
        } else {
            assert_eq!((verdict, told), (Some(1), format!("ironwork: RETURN-CODE {rc} exits 1\n")), "{rc}");
        }
    }
}

#[test]
fn a_return_code_outside_0_to_238_or_of_239_exits_239_and_standard_error_and_the_journal_give_it() {
    let dir = Dir::new("239");
    for rc in [239, 240, 255, 256, 1000, -1] {
        let ((band, said), (verdict, told)) = both(&["run", &returning(&dir, &rc.to_string())]);
        assert_eq!((band, said), (Some(239), format!("ironwork: RETURN-CODE {rc} exits 239\n")), "{rc}");
        assert_eq!((verdict, told), (Some(1), format!("ironwork: RETURN-CODE {rc} exits 1\n")), "{rc}");
    }
    let out = ironwork(&["run", &returning(&dir, "1000"), "--evidence", &dir.path("ev")]);
    assert_eq!(out.status.code(), Some(239));
    let runs = fs::read_dir(dir.0.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let close = fs::read_to_string(runs).unwrap().lines().last().unwrap().to_owned();
    assert!(close.contains("\"kind\":\"close\"") && close.contains("\"exit\":1000,"), "{close}");
}

#[test]
fn an_abend_exits_240_and_with_exit_code_3() {
    let dir = Dir::new("abend");
    let program = dir.program("ABENDS", &DATA_EXCEPTION, &["ADD WS-N TO WS-T.", "GOBACK."]);
    let ((band, said), (verdict, _)) = both(&["run", &program]);
    assert_eq!((band, verdict), (Some(240), Some(3)), "{said}");
    assert!(said.contains(": ABEND S0C7: "), "{said}");
    let ((band, said), (verdict, _)) = both(&["cics", &program]);
    assert_eq!((band, verdict), (Some(240), Some(3)), "{said}");
    assert!(said.contains(": ABEND ASRA: "), "{said}");
    let out = dir.job("//S1 EXEC PGM=ABENDS\n//S2 EXEC PGM=IEFBR14\n", &[]);
    assert_eq!(out.status.code(), Some(240), "{}", stderr(&out));
    let out = dir.job("//S1 EXEC PGM=IEFBR14\n//IN DD DSN=NOT.THERE,DISP=SHR\n", &["--exit-code"]);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
}

#[test]
fn a_program_the_compile_refuses_exits_241_with_the_compile_s_return_code_and_with_exit_code_4() {
    let dir = Dir::new("refused");
    let program = dir.program("BROKEN", &["01 X PIC X."], &["MOVE Y TO X.", "GOBACK."]);
    for command in ["run", "cics"] {
        let ((band, said), (verdict, _)) = both(&[command, &program]);
        assert_eq!((band, verdict), (Some(241), Some(4)), "{command}");
        assert!(said.ends_with(&format!("ironwork: {program}: the program does not run: the compile's return code is 12\n")), "{command}: {said}");
    }
    assert_eq!(ironwork(&["check", &program]).status.code(), Some(12));
    let out = dir.job("//S1 EXEC PGM=BROKEN\n", &[]);
    assert_eq!(out.status.code(), Some(241), "{}", stderr(&out));
}

#[test]
fn a_construct_code_generation_refuses_exits_242_and_with_exit_code_4() {
    let dir = Dir::new("lowering");
    let program = dir.program("MIXED", &["01 N PIC 9(3) VALUE 5."], &["IF N = ALL ZERO DISPLAY 'Z' END-IF.", "STOP RUN."]);
    let text = fs::read_to_string(&program).unwrap();
    fs::write(&program, format!("       CBL NUMCHECK\n{text}")).unwrap();
    for command in ["run", "cics"] {
        let ((band, said), (verdict, _)) = both(&[command, &program, "--vm"]);
        assert_eq!((band, verdict), (Some(242), Some(4)), "{command}: {said}");
        assert!(said.contains(": IWR0052-S lowering: NUMCHECK with ALL ZERO or ALL NULL compared with a data item it may test is not lowered yet"), "{command}: {said}");
    }
    assert_eq!(ironwork(&["compile", &program, "-o", &dir.path("out")]).status.code(), Some(12));
    // Without --vm the program runs on the interpreter, with a line naming what was refused.
    let out = ironwork(&["run", &program]);
    let said = stderr(&out);
    assert_eq!((out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()), (Some(0), ""), "{said}");
    assert!(said.contains(&format!("ironwork: {program}:")) && said.contains(": runs on the interpreter (lowering: NUMCHECK with ALL ZERO"), "{said}");
}

#[test]
fn a_construct_the_vm_does_not_run_yet_exits_243_and_with_exit_code_5() {
    let dir = Dir::new("stopped");
    let program = dir.program("UUID", &["01 U PIC X(36)."], &["MOVE FUNCTION UUID4 TO U.", "GOBACK."]);
    let ((band, said), (verdict, _)) = both(&["run", &program, "--vm"]);
    assert_eq!((band, verdict), (Some(243), Some(5)), "{said}");
    assert!(said.contains("the VM does not run FUNCTION UUID4"), "{said}");
    assert_eq!(ironwork(&["compile", &program, "-o", &dir.path("out")]).status.code(), Some(0));
    let ((band, said), (verdict, _)) = both(&["run", &dir.path("out/UUID.iwm")]);
    assert_eq!((band, verdict), (Some(243), Some(5)), "{said}");
}

#[test]
fn a_construct_ironwork_does_not_run_exits_244_and_with_exit_code_4() {
    let dir = Dir::new("not-run");
    let dli = dir.program("DLI", &["01 AREA1 PIC X(80)."], &["EXEC DLI GN SEGMENT(ROOT) INTO(AREA1) END-EXEC.", "GOBACK."]);
    let ((band, said), (verdict, _)) = both(&["run", &dli]);
    assert_eq!((band, verdict), (Some(244), Some(4)), "{said}");
    assert!(said.contains(": ABEND EXEC: EXEC DLI GN was reached"), "{said}");
    let client = dir.program("CLIENT", &["01 A1 USAGE OBJECT REFERENCE Account."], &["INVOKE A1 \"open\".", "GOBACK."]);
    let repository = "       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n       DATA DIVISION.\n";
    let text = fs::read_to_string(&client).unwrap().replace("       DATA DIVISION.\n", repository);
    fs::write(&client, text).unwrap();
    let ((band, said), (verdict, _)) = both(&["cics", &client]);
    assert_eq!((band, verdict), (Some(244), Some(4)), "{said}");
    assert!(said.contains(": ABEND IRONWORK: INVOKE was reached in a CICS task"), "{said}");
    let out = dir.job("//S1 EXEC PGM=ICETOOL\n", &[]);
    assert_eq!(out.status.code(), Some(244), "{}", stderr(&out));
    assert!(stderr(&out).contains("PGM=ICETOOL is not supported yet"), "{}", stderr(&out));
}

#[test]
fn a_source_jcl_or_module_that_cannot_be_read_exits_245_and_with_exit_code_2() {
    let dir = Dir::new("unreadable");
    for command in ["run", "cics"] {
        let ((band, said), (verdict, _)) = both(&[command, &dir.path("src/NOWHERE.cbl")]);
        assert_eq!((band, verdict), (Some(245), Some(2)), "{command}: {said}");
    }
    assert_eq!(ironwork(&["check", &dir.path("src/NOWHERE.cbl")]).status.code(), Some(2));
    fs::write(dir.0.join("src/BAD.iwm"), b"not a module").unwrap();
    let ((band, said), (verdict, _)) = both(&["run", &dir.path("src/BAD.iwm")]);
    assert_eq!((band, verdict), (Some(245), Some(2)), "{said}");
    let out = ironwork(&["job", &dir.path("NOWHERE.jcl"), "--datasets", &dir.path("data")]);
    assert_eq!(out.status.code(), Some(245), "{}", stderr(&out));
}

#[test]
fn usage_exits_246_for_run_cics_and_job_and_with_exit_code_2_and_2_for_the_other_commands() {
    let dir = Dir::new("usage");
    let program = returning(&dir, "0");
    for command in ["run", "cics"] {
        let ((band, _), (verdict, _)) = both(&[command, &program, "--bogus"]);
        assert_eq!((band, verdict), (Some(246), Some(2)), "{command}");
    }
    assert_eq!(ironwork(&["--bogus", "run", &program]).status.code(), Some(246), "a flag before the command");
    assert_eq!(ironwork(&["job", &dir.path("job.jcl")]).status.code(), Some(246), "job with no --datasets");
    for args in [&["check", &program, "--bogus"][..], &["check", &program, "--exit-code"], &["compile", &program, "--exit-code"], &["assumptions", "--exit-code"], &[]] {
        assert_eq!(ironwork(args).status.code(), Some(2), "{args:?}");
    }
    let out = dir.job("//S1 EXEC PGM=IEFBR14\n", &["--exit-code", "--expected", &format!("DATASETS={}", dir.path("data"))]);
    assert_eq!(out.status.code(), Some(2), "--exit-code is not for an equivalence: {}", stderr(&out));
}

#[test]
fn a_job_exits_with_its_highest_step_return_code_and_says_one_outside_0_to_238() {
    let dir = Dir::new("job");
    dir.program("FOUR", &[], &["MOVE 4 TO RETURN-CODE.", "GOBACK."]);
    dir.program("BIG", &[], &["MOVE 1000 TO RETURN-CODE.", "GOBACK."]);
    let out = dir.job("//S1 EXEC PGM=FOUR\n//S2 EXEC PGM=IEFBR14\n", &[]);
    assert_eq!((out.status.code(), stderr(&out).contains("exits")), (Some(4), false), "{}", stderr(&out));
    let out = dir.job("//S1 EXEC PGM=FOUR\n", &["--exit-code"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).ends_with("ironwork: the highest step return code 4 exits 1\n"), "{}", stderr(&out));
    let out = dir.job("//S1 EXEC PGM=BIG\n//S2 EXEC PGM=FOUR\n", &[]);
    assert_eq!(out.status.code(), Some(239));
    assert!(stderr(&out).ends_with("ironwork: the highest step return code 1000 exits 239\n"), "{}", stderr(&out));
}

#[test]
fn a_job_exits_as_its_first_step_that_ended_without_a_return_code_says() {
    let dir = Dir::new("job-first");
    dir.program("ABENDS", &DATA_EXCEPTION, &["ADD WS-N TO WS-T.", "GOBACK."]);
    dir.program("DLI", &["01 AREA1 PIC X(80)."], &["EXEC DLI GN SEGMENT(ROOT) INTO(AREA1) END-EXEC.", "GOBACK."]);
    let out = dir.job("//S1 EXEC PGM=ABENDS\n//S2 EXEC PGM=DLI,COND=EVEN\n", &[]);
    assert_eq!(out.status.code(), Some(240), "{}", stderr(&out));
    let out = dir.job("//S1 EXEC PGM=DLI\n//S2 EXEC PGM=ABENDS,COND=EVEN\n", &[]);
    assert_eq!(out.status.code(), Some(244), "{}", stderr(&out));
    let log = stderr(&out);
    assert!(log.contains("S1 PGM=DLI ABEND EXEC") && log.contains("S2 PGM=ABENDS ABEND S0C7"), "{log}");
}

#[test]
fn the_tables_in_the_help_and_the_readme_give_each_verdict_the_codes_it_stands_for() {
    let help = stderr(&ironwork(&["--bogus"]));
    let readme = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md")).unwrap();
    let verdicts = [(0, ""), (1, ""), (2, "(246, 245)"), (3, "(240)"), (4, "(241, 242, 244)"), (5, "(243)"), (70, "(255)")];
    let help_rows = help.replace("\n         ", " ");
    for (verdict, band) in verdicts {
        assert!(help_rows.lines().any(|l| l.starts_with(&format!("  {verdict:<7}")) && l.ends_with(band)), "help: {verdict} {band}");
        assert!(readme.lines().any(|l| l.starts_with(&format!("| {verdict} | ")) && l.trim_end_matches(" |").trim_end_matches('.').ends_with(band)), "README: {verdict} {band}");
    }
    for band in ["0-238", "239", "240", "241", "242", "243", "244", "245", "246", "255"] {
        assert!(help_rows.lines().any(|l| l.starts_with(&format!("  {band:<7}"))), "help: {band}");
        assert!(readme.contains(&format!("| {} | ", band.replace('-', "–"))), "README: {band}");
    }
}
