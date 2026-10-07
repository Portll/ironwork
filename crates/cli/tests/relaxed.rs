//! `--compliance relaxed`: what extended refuses in a PROCEDURE DIVISION sentence or statement
//! compiles as a hole, and a run ends with IWR0078 where it reaches one, on both executors and from a
//! load module.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-relaxed-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// An IF whose body names no data item, a sentence that does not parse, and a GO TO of no paragraph.
fn program(dir: &std::path::Path, x: u8) -> String {
    let text = format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HOLES.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 X PIC 9 VALUE {x}.\n       PROCEDURE DIVISION.\n           DISPLAY 'BEFORE'.\n           IF X = 2\n               MOVE NOSUCH TO X\n           END-IF.\n           IF X = 3\n               GO TO NOWHERE\n           END-IF.\n           DISPLAY 'AFTER'.\n           FROB X WIBBLE.\n           GOBACK.\n"
    );
    let path = dir.join(format!("HOLES{x}.cbl"));
    fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn check_names_each_hole_and_extended_refuses_what_relaxed_compiles() {
    let dir = temp("check");
    let path = program(&dir, 1);
    let o = ironwork(&["check", &path, "--compliance", "relaxed"]);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    let holes: Vec<String> = stderr(&o).lines().filter(|l| l.contains("IWX0059-W")).map(|l| l.split(": warning:").next().unwrap().rsplit(['/', '\\']).next().unwrap().to_owned()).collect();
    assert_eq!(holes, ["HOLES1.cbl:15:12", "HOLES1.cbl:9:21", "HOLES1.cbl:12:16"], "{}", stderr(&o));
    assert!(stderr(&o).contains("the sentence at line 15 (--compliance relaxed): IWS0001-S a statement, found FROB"), "{}", stderr(&o));
    assert_eq!(ironwork(&["check", &path, "--compliance=extended"]).status.code(), Some(12));
    assert_eq!(ironwork(&["check", &path, "--compliance=relaxed"]).status.code(), Some(4));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_run_ends_at_the_first_hole_it_reaches_on_either_executor_and_from_a_module() {
    let dir = temp("run");
    for (x, shown, hole) in [(1, "BEFORE\nAFTER\n", 15), (2, "BEFORE\n", 9), (3, "BEFORE\n", 12)] {
        let path = program(&dir, x);
        for executor in ["--vm", "--interpret"] {
            let o = ironwork(&["run", &path, "--compliance", "relaxed", executor]);
            assert_eq!(stdout(&o), shown, "{x} {executor}: {}", stderr(&o));
            assert_eq!(o.status.code(), Some(244), "{x} {executor}: {}", stderr(&o));
            assert!(stderr(&o).contains(&format!(":{hole}:")) && stderr(&o).contains("ABEND IRONWORK: IWR0078-S"), "{x} {executor}: {}", stderr(&o));
        }
        let out = dir.join(format!("out{x}"));
        assert_eq!(ironwork(&["compile", &path, "--compliance", "relaxed", "-o", out.to_str().unwrap()]).status.code(), Some(4));
        let module = out.join(format!("HOLES{x}.iwm"));
        let o = ironwork(&["run", module.to_str().unwrap()]);
        assert_eq!((stdout(&o).as_str(), o.status.code()), (shown, Some(244)), "{x} module: {}", stderr(&o));
    }
    fs::remove_dir_all(dir).unwrap();
}
