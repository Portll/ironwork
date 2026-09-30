use std::process::Command;

fn ironwork(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

#[test]
fn fastsrt_adv_print_takes_exclude_or_include() {
    let program = std::env::temp_dir().join(format!("ironwork-fastsrt-adv-print-{}.cbl", std::process::id()));
    std::fs::write(&program, "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           GOBACK.\n").unwrap();
    let path = program.to_str().unwrap();
    for choice in ["--fastsrt-adv-print=exclude", "--fastsrt-adv-print=include"] {
        let out = ironwork(&["check", path, choice]);
        assert!(out.status.success(), "{choice}: {}", String::from_utf8_lossy(&out.stderr));
    }
    for bad in ["--fastsrt-adv-print", "--fastsrt-adv-print=maybe", "--fastsrt-adv-print=INCLUDE"] {
        let out = ironwork(&["check", path, bad]);
        assert_eq!(out.status.code(), Some(2), "{bad}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("--fastsrt-adv-print needs =exclude or =include"), "{bad}");
    }
    let _ = std::fs::remove_file(&program);
}
