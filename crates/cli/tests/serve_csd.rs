//! `ironwork cics --serve --csd`: the region's transactions come from cobolwork's shared definition
//! (fixtures/cobolwork/csd/entry.csd), where MNU1 runs MENU and INQ1 runs INQUIRY.

mod common;

use common::{converse, serve};
use std::path::Path;

const MENU: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MENU.\n       PROCEDURE DIVISION.\n           DISPLAY 'MENU'.\n           EXEC CICS RETURN TRANSID('INQ1') END-EXEC.\n";
const INQUIRY: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INQUIRY.\n       PROCEDURE DIVISION.\n           DISPLAY 'INQUIRY'.\n           EXEC CICS RETURN END-EXEC.\n";

#[test]
fn a_served_region_runs_the_transactions_its_csd_defines() {
    let dir = std::env::temp_dir().join(format!("ironwork-serve-csd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("MENU.cbl"), MENU).unwrap();
    std::fs::write(dir.join("INQUIRY.cbl"), INQUIRY).unwrap();
    let csd = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cobolwork/csd/entry.csd");
    let server = serve(&dir.join("MENU.cbl"), &["-L", dir.to_str().unwrap(), "--transid", "MNU1", "--csd", csd.to_str().unwrap()]);
    assert_eq!(converse(&server), (vec!["MENU".to_owned(), "INQUIRY".to_owned()], 2));
    drop(server);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_csd_needs_serve_and_a_broken_one_is_refused() {
    let run = |args: &[&str]| std::process::Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap();
    let dir = std::env::temp_dir().join(format!("ironwork-csd-refused-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("MENU.cbl"), MENU).unwrap();
    std::fs::write(dir.join("BAD.csd"), "* no name\n DEFINE TRANSACTION GROUP(X)\n").unwrap();
    let program = dir.join("MENU.cbl");
    let alone = run(&["cics", program.to_str().unwrap(), "--csd", "x.csd"]);
    assert_eq!(alone.status.code(), Some(246));
    assert!(String::from_utf8_lossy(&alone.stderr).contains("--csd need --serve"));
    let broken = run(&["cics", program.to_str().unwrap(), "--serve", "127.0.0.1:0", "--csd", dir.join("BAD.csd").to_str().unwrap()]);
    assert_eq!(broken.status.code(), Some(246));
    assert!(String::from_utf8_lossy(&broken.stderr).contains("BAD.csd: 2:1: DEFINE names no KIND(NAME)"), "{}", String::from_utf8_lossy(&broken.stderr));
    std::fs::remove_dir_all(dir).unwrap();
}
