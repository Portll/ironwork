//! `--compliance strict|extended`: strict refuses what Enterprise COBOL refuses; extended reads the
//! other dialects' extensions docs/compliance.md lists, each with a warning, in every command that
//! compiles, and the level is recorded where the options are.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-compliance-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

fn ironwork(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir).args(args).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A free-form program with a level-78 constant, <> and a concatenation.
const FREE: &str = concat!(
    "IDENTIFICATION DIVISION.\n",
    "PROGRAM-ID. FREEPGM.\n",
    "DATA DIVISION.\n",
    "WORKING-STORAGE SECTION.\n",
    "78 LIMIT VALUE 3.\n",
    "01 WS-N PIC 9 VALUE 0.\n",
    "PROCEDURE DIVISION.\n",
    "    PERFORM UNTIL WS-N = LIMIT\n",
    "        ADD 1 TO WS-N\n",
    "    END-PERFORM\n",
    "    IF WS-N <> 0\n",
    "        DISPLAY \"COUNTED \" & \"TO \" WS-N\n",
    "    END-IF\n",
    "    GOBACK.\n",
);

const WARNINGS: [&str; 4] = [
    "FREEPGM.cbl:1:7: warning: IWX0001-W free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): column 7 holds 'F', which no fixed-form line can, so the file is read in free form",
    "FREEPGM.cbl:5:1: warning: IWX0002-W constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): LIMIT stands for its value wherever it is used after this entry",
    "FREEPGM.cbl:11:13: warning: IWX0003-W <> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =",
    "FREEPGM.cbl:12:28: warning: IWX0004-W literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal",
];

fn warnings() -> String {
    WARNINGS.map(|w| format!("{w}\n")).concat()
}

#[test]
fn check_refuses_free_form_source_under_strict_and_lists_each_extension_under_extended() {
    let dir = temp("check");
    fs::write(dir.join("FREEPGM.cbl"), FREE).unwrap();
    let strict = ironwork(&dir, &["check", "FREEPGM.cbl"]);
    assert_eq!(strict.status.code(), Some(12));
    assert!(!text(&strict.stderr).contains("IWX"), "{}", text(&strict.stderr));
    let named = ironwork(&dir, &["check", "FREEPGM.cbl", "--compliance", "strict"]);
    assert_eq!((named.status.code(), text(&named.stderr)), (strict.status.code(), text(&strict.stderr)));
    for flag in [&["--compliance", "extended"][..], &["--compliance=extended"]] {
        let extended = ironwork(&dir, &[&["check", "FREEPGM.cbl"][..], flag].concat());
        assert_eq!((extended.status.code(), text(&extended.stderr)), (Some(4), warnings()), "{flag:?}");
    }
    let ran = ironwork(&dir, &["run", "FREEPGM.cbl", "--compliance", "extended"]);
    assert_eq!((ran.status.code(), text(&ran.stdout)), (Some(0), "COUNTED TO 3\n".to_string()), "{}", text(&ran.stderr));
    let vm = ironwork(&dir, &["run", "--vm", "FREEPGM.cbl", "--compliance", "extended"]);
    assert_eq!((vm.status.code(), text(&vm.stdout)), (Some(0), "COUNTED TO 3\n".to_string()), "{}", text(&vm.stderr));
    let task = ironwork(&dir, &["cics", "FREEPGM.cbl", "--compliance", "extended"]);
    assert_eq!((task.status.code(), text(&task.stdout)), (Some(0), "COUNTED TO 3\n".to_string()), "{}", text(&task.stderr));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_level_that_is_not_strict_or_extended_is_a_usage_error_and_dump_takes_none() {
    let dir = temp("usage");
    fs::write(dir.join("FREEPGM.cbl"), FREE).unwrap();
    for args in [&["check", "FREEPGM.cbl", "--compliance", "mf"][..], &["check", "FREEPGM.cbl", "--compliance=EXTENDED"], &["check", "FREEPGM.cbl", "--compliance"]] {
        let o = ironwork(&dir, args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(text(&o.stderr).starts_with("ironwork: --compliance needs strict, extended or relaxed\n"), "{}", text(&o.stderr));
    }
    let dumped = ironwork(&dir, &["dump", "X.iwm", "--compliance", "extended"]);
    assert_eq!(dumped.status.code(), Some(2));
    assert!(text(&dumped.stderr).starts_with("ironwork: dump takes --section, --strings and --no-check"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_module_carries_the_level_and_a_called_program_is_read_under_it() {
    let dir = temp("module");
    fs::write(dir.join("FREEPGM.cbl"), FREE).unwrap();
    assert_eq!(ironwork(&dir, &["compile", "FREEPGM.cbl", "-o", "out"]).status.code(), Some(12));
    let compiled = ironwork(&dir, &["compile", "FREEPGM.cbl", "-o", "out", "--compliance", "extended"]);
    assert_eq!(compiled.status.code(), Some(4), "{}", text(&compiled.stderr));
    let dumped = ironwork(&dir, &["dump", "--section", "OPTIONS", "out/FREEPGM.iwm"]);
    assert!(text(&dumped.stdout).lines().any(|l| l == "FREEPGM compliance: Extended"), "{}", text(&dumped.stdout));
    let main = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAINPGM.\n       PROCEDURE DIVISION.\n           CALL 'FREEPGM'\n           GOBACK.\n";
    fs::write(dir.join("MAINPGM.cbl"), main).unwrap();
    fs::rename(dir.join("FREEPGM.cbl"), dir.join("lib/FREEPGM.cbl")).unwrap();
    let strict = ironwork(&dir, &["run", "MAINPGM.cbl", "-L", "lib"]);
    assert_eq!(strict.status.code(), Some(244), "{}", text(&strict.stderr));
    let called = ironwork(&dir, &["run", "MAINPGM.cbl", "-L", "lib", "--compliance", "extended"]);
    assert_eq!((called.status.code(), text(&called.stdout)), (Some(0), "COUNTED TO 3\n".to_string()), "{}", text(&called.stderr));
    for (level, code) in [("strict", Some(244)), ("extended", Some(0))] {
        let compiled = ironwork(&dir, &["compile", "MAINPGM.cbl", "-o", level, "--compliance", level]);
        assert_eq!(compiled.status.code(), Some(0), "{}", text(&compiled.stderr));
        let module = ironwork(&dir, &["run", &format!("{level}/MAINPGM.iwm"), "-L", "lib"]);
        assert_eq!(module.status.code(), code, "{level}: {}", text(&module.stderr));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn evidence_keeps_the_level_and_provenance_names_it_among_the_options_in_force() {
    let dir = temp("evidence");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/FREEPGM.cbl"), FREE).unwrap();
    let o = ironwork(&dir, &["check", "src/FREEPGM.cbl", "--compliance", "extended", "--provenance", "prov.json", "--evidence", "ev"]);
    assert_eq!(o.status.code(), Some(4), "{}", text(&o.stderr));
    let provenance = fs::read_to_string(dir.join("prov.json")).unwrap();
    assert!(provenance.contains("\"compliance\":\"extended\""), "{provenance}");
    assert!(provenance.contains("\"flags\":[\"--compliance=extended\"]"), "{provenance}");
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let journal = fs::read_to_string(run).unwrap();
    assert!(journal.contains("\"--compliance\",\"extended\""), "{journal}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn job_and_fuzz_compile_their_programs_under_the_level() {
    let dir = temp("job");
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(dir.join("lib/FREEPGM.cbl"), FREE).unwrap();
    fs::write(dir.join("job.jcl"), "//TESTJOB JOB (1),'T',CLASS=A\n//S1 EXEC PGM=FREEPGM\n").unwrap();
    let job = |extra: &[&str]| ironwork(&dir, &[&["job", "job.jcl", "--datasets", "data:text", "-L", "lib"][..], extra].concat());
    let strict = job(&[]);
    assert_ne!(strict.status.code(), Some(0), "{}", text(&strict.stderr));
    let extended = job(&["--compliance", "extended"]);
    assert_eq!((extended.status.code(), text(&extended.stdout)), (Some(0), "COUNTED TO 3\n".to_string()), "{}", text(&extended.stderr));
    let sysin = FREE.replace("    PERFORM UNTIL", "    ACCEPT WS-N\n    COMPUTE WS-N = LIMIT / WS-N\n    PERFORM UNTIL");
    fs::create_dir_all(dir.join("repo")).unwrap();
    fs::write(dir.join("repo/SYSPGM.cbl"), sysin.replace("FREEPGM", "SYSPGM")).unwrap();
    let fuzz = |out: &str, extra: &[&str]| ironwork(&dir, &[&["fuzz", "repo/SYSPGM.cbl", "-o", out, "--runs", "60"][..], extra].concat());
    assert_ne!(fuzz("strict", &[]).status.code(), Some(0));
    let fuzzed = fuzz("extended", &["--compliance", "extended"]);
    assert_eq!(fuzzed.status.code(), Some(0), "{}", text(&fuzzed.stderr));
    let manifest = fs::read_to_string(dir.join("extended/manifest.json")).unwrap();
    assert!(manifest.contains("\"code\":\"S0C"), "{manifest}");
    fs::remove_dir_all(dir).unwrap();
}
