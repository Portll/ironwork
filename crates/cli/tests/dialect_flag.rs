//! `--dialect ibm|gnucobol` on each command that compiles: the result it changes, and where the
//! choice is recorded.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-dialect-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

/// A quotient inside a ROUNDED COMPUTE: 5.12 under ibm, 5.11 under gnucobol (assumption C101).
fn program(dir: &Path) -> String {
    let lines = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. ROUNDS.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  D PIC 9(5)V99 VALUE 1.",
        "01  E PIC 9(5)V99 VALUE 12.35.",
        "PROCEDURE DIVISION.",
        "    COMPUTE D ROUNDED = D + E / 3",
        "    DISPLAY D",
        "    GOBACK.",
    ];
    let path = dir.join("lib/ROUNDS.cbl");
    fs::write(&path, lines.iter().map(|l| format!("       {l}\n")).collect::<String>()).unwrap();
    path.to_str().unwrap().to_owned()
}

fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn run_takes_the_dialect_in_either_spelling_on_both_executors() {
    let dir = temp("run");
    let path = program(&dir);
    assert_eq!(stdout(&ironwork(&["run", &path])), "0000512\n");
    for executor in [&[][..], &["--vm"][..]] {
        for (args, shown) in [(&["--dialect", "ibm"][..], "0000512\n"), (&["--dialect", "gnucobol"], "0000511\n"), (&["--dialect=gnucobol"], "0000511\n")] {
            let argv: Vec<&str> = ["run", path.as_str()].iter().copied().chain(executor.iter().copied()).chain(args.iter().copied()).collect();
            assert_eq!(stdout(&ironwork(&argv)), shown, "{argv:?}");
        }
    }
    for bad in [&["--dialect"][..], &["--dialect", "mf"], &["--dialect=GNUCOBOL"], &["--dialect="]] {
        let argv: Vec<&str> = ["check", path.as_str()].iter().copied().chain(bad.iter().copied()).collect();
        let o = ironwork(&argv);
        assert_eq!(o.status.code(), Some(2), "{bad:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("--dialect needs ibm or gnucobol"), "{bad:?}");
        let ran = ironwork(&["run", path.as_str(), bad[0]].iter().copied().chain(bad[1..].iter().copied()).collect::<Vec<_>>());
        assert_eq!(ran.status.code(), Some(246), "{bad:?}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn check_cics_job_and_compile_take_the_dialect_and_dump_refuses_it() {
    let dir = temp("commands");
    let path = program(&dir);
    assert!(ironwork(&["check", &path, "--dialect", "gnucobol"]).status.success());
    assert_eq!(stdout(&ironwork(&["cics", &path, "--dialect", "gnucobol"])), "0000511\n");
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(dir.join("job.jcl"), "//DIALECT JOB\n//STEP1 EXEC PGM=ROUNDS\n").unwrap();
    let job = |dialect: &str| {
        let o = Command::new(env!("CARGO_BIN_EXE_ironwork"))
            .arg("job")
            .arg(dir.join("job.jcl"))
            .arg("--datasets")
            .arg(dir.join("data"))
            .arg("-L")
            .arg(dir.join("lib"))
            .args(["--dialect", dialect])
            .output()
            .unwrap();
        stdout(&o)
    };
    assert!(job("ibm").contains("0000512\n"));
    assert!(job("gnucobol").contains("0000511\n"));
    let out = dir.join("out");
    assert!(ironwork(&["compile", &path, "-o", out.to_str().unwrap(), "--dialect", "gnucobol"]).status.success());
    let module = out.join("ROUNDS.iwm");
    let dumped = stdout(&ironwork(&["dump", "--section", "OPTIONS", module.to_str().unwrap()]));
    assert!(dumped.lines().any(|l| l == "ROUNDS dialect: Gnucobol"), "{dumped}");
    let o = ironwork(&["dump", module.to_str().unwrap(), "--dialect", "gnucobol"]);
    assert_eq!(o.status.code(), Some(2));
    let o = ironwork(&["fuzz", &path, "-o", dir.join("fuzz").to_str().unwrap(), "--root", dir.to_str().unwrap(), "--dialect", "gnucobol"]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("nothing to vary"), "fuzz reads the flag and then the program");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provenance_and_the_journal_record_the_dialect() {
    let dir = temp("recorded");
    let path = program(&dir);
    let (prov, ev) = (dir.join("prov.json"), dir.join("ev"));
    let o = ironwork(&["run", &path, "--dialect", "gnucobol", "--provenance", prov.to_str().unwrap(), "--evidence", ev.to_str().unwrap()]);
    assert_eq!(stdout(&o), "0000511\n");
    let statement = fs::read_to_string(&prov).unwrap();
    assert!(statement.contains("\"flags\":[\"--dialect=gnucobol\"]"), "{statement}");
    assert!(statement.contains("\"dialect\":\"gnucobol\""), "{statement}");
    let run = fs::read_dir(ev.join("runs")).unwrap().next().unwrap().unwrap().path();
    let open = fs::read_to_string(run).unwrap().lines().next().unwrap().to_owned();
    assert!(open.contains("\"argv\":[\"run\",\"--dialect\",\"gnucobol\""), "{open}");
    let ibm = dir.join("ibm.json");
    assert_eq!(stdout(&ironwork(&["check", &path, "--provenance", ibm.to_str().unwrap()])), "");
    assert!(fs::read_to_string(&ibm).unwrap().contains("\"dialect\":\"ibm\""));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_target_stands_for_its_flags_and_the_flags_beside_it_override_them() {
    let dir = temp("target");
    let path = dir.join("RETMAIN.cbl");
    let text = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. RETMAIN.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 N PIC S9(3) VALUE -12.\n       PROCEDURE DIVISION.\n           MOVE 7 TO RETURN-CODE\n           DISPLAY N.\n";
    fs::write(&path, text).unwrap();
    let path = path.to_str().unwrap();
    let run = |args: &[&str]| {
        let o = ironwork(&[&["run", path], args].concat());
        (String::from_utf8_lossy(&o.stdout).into_owned(), o.status.code())
    };
    assert_eq!(run(&["--target", "gnucobol"]), ("-012\n".to_owned(), Some(7)));
    assert_eq!(run(&["--target=gnucobol-ibm-strict"]), ("012-\n".to_owned(), Some(7)));
    assert_eq!(run(&["--target", "ibm"]).0, "01K\n");
    assert_eq!(run(&["--numeric-display", "ibm", "--target", "gnucobol"]).0, "01K\n");
    assert_eq!(ironwork(&["check", path, "--target", "mf"]).status.code(), Some(2));
    fs::remove_dir_all(dir).unwrap();
}
