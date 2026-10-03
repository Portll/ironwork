use std::process::Command;

fn ironwork(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

#[test]
fn c_series_belongs_to_assumptions() {
    let plain = ironwork(&["assumptions"]);
    let numbered = ironwork(&["assumptions", "--c-series"]);
    assert!(plain.status.success() && numbered.status.success());
    assert!(String::from_utf8_lossy(&numbered.stdout).starts_with("C1\t"));
}

#[test]
fn other_commands_refuse_c_series() {
    for (command, usage) in [("run", 246), ("check", 2), ("cics", 246)] {
        let out = ironwork(&[command, "--c-series", "missing.cbl"]);
        assert_eq!(out.status.code(), Some(usage), "{command}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("unknown flag --c-series"), "{command}");
    }
}
