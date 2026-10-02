//! `ironwork cics --serve`: the server asks for no credentials, so it serves a loopback address
//! unless --serve-public is given.

use std::process::Command;

const MENU: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MENU.\n       PROCEDURE DIVISION.\n           DISPLAY 'MENU'.\n           EXEC CICS RETURN END-EXEC.\n";

#[test]
fn an_address_beyond_loopback_is_refused_without_serve_public() {
    let dir = std::env::temp_dir().join(format!("ironwork-serve-address-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("MENU.cbl"), MENU).unwrap();
    let program = dir.join("MENU.cbl");
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("cics").arg(&program).args(args).output().unwrap();

    let open = run(&["--serve", "0.0.0.0:0"]);
    let said = String::from_utf8_lossy(&open.stderr);
    assert_eq!(open.status.code(), Some(2), "{said}");
    assert!(said.contains("0.0.0.0 is not a loopback address") && said.contains("--serve-public"), "{said}");

    let stray = run(&["--serve-public"]);
    assert_eq!(stray.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&stray.stderr).contains("--serve-public is for --serve"));
    std::fs::remove_dir_all(dir).unwrap();
}
