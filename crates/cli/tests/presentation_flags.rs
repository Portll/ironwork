//! `--numeric-display` and `--empty-literal`: what each value gives, the default each compliance
//! level takes, a load module carrying the display form, and the values each refuses.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-presentation-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn program(dir: &std::path::Path) -> String {
    let lines = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. SHOWN.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01 A PIC S9(3)V99 VALUE -12.5.",
        "01 X PIC X(3) VALUE 'abc'.",
        "PROCEDURE DIVISION.",
        "    MOVE '' TO X",
        "    DISPLAY A '[' X ']'",
        "    GOBACK.",
    ];
    let path = dir.join("SHOWN.cbl");
    fs::write(&path, lines.iter().map(|l| format!("       {l}\n")).collect::<String>()).unwrap();
    path.to_str().unwrap().to_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn each_level_shows_its_form_and_the_flags_choose_another() {
    let dir = temp("forms");
    let path = program(&dir);
    for (args, shown) in [
        (&[][..], "0125}[   ]\n"),
        (&["--compliance", "extended"], "-012.50[   ]\n"),
        (&["--compliance", "relaxed"], "-012.50[   ]\n"),
        (&["--dialect", "gnucobol"], "01250-[   ]\n"),
        (&["--compliance=extended", "--numeric-display", "ibm"], "0125}[   ]\n"),
        (&["--numeric-display=cobc-ibm-strict"], "01250-[   ]\n"),
        (&["--compliance=extended", "--empty-literal", "empty"], "-012.50[   ]\n"),
    ] {
        let argv: Vec<&str> = ["run", path.as_str()].iter().copied().chain(args.iter().copied()).collect();
        let o = ironwork(&argv);
        assert_eq!(stdout(&o), shown, "{args:?}: {}", stderr(&o));
    }
    let strict = ironwork(&["check", &path]);
    assert_eq!(strict.status.code(), Some(8), "{}", stderr(&strict));
    assert!(stderr(&strict).contains("IWS0106-E ''"), "{}", stderr(&strict));
    let out = dir.join("out");
    assert_eq!(ironwork(&["compile", &path, "--compliance", "extended", "-o", out.to_str().unwrap()]).status.code(), Some(4));
    let module = ironwork(&["run", out.join("SHOWN.iwm").to_str().unwrap()]);
    assert_eq!(stdout(&module), "-012.50[   ]\n", "{}", stderr(&module));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn each_flag_refuses_a_value_it_does_not_take() {
    let dir = temp("refused");
    let path = program(&dir);
    for (bad, said) in [
        (&["--numeric-display", "pretty"][..], "--numeric-display needs ibm, cobc-ibm-strict or cobc"),
        (&["--numeric-display=COBC"], "--numeric-display needs ibm, cobc-ibm-strict or cobc"),
        (&["--empty-literal", "null"], "--empty-literal needs space or empty"),
        (&["--empty-literal"], "--empty-literal needs space or empty"),
    ] {
        let argv: Vec<&str> = ["check", path.as_str()].iter().copied().chain(bad.iter().copied()).collect();
        let o = ironwork(&argv);
        assert_eq!(o.status.code(), Some(2), "{bad:?}");
        assert!(stderr(&o).contains(said), "{bad:?}: {}", stderr(&o));
    }
    fs::remove_dir_all(dir).unwrap();
}
