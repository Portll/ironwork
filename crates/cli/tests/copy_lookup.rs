use std::process::Command;

fn ironwork(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn program(id: &str, section: &str, copy: &str, statement: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       DATA DIVISION.\n       {section} SECTION.\n       COPY {copy}.\n       PROCEDURE DIVISION.\n{statement}           GOBACK.\n"
    )
}

#[test]
fn a_program_takes_its_copybook_and_never_its_own_source() {
    let root = std::env::temp_dir().join(format!("ironwork-copy-lookup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let files = [
        ("src/PGMA.cbl", program("PGMA", "LINKAGE", "PGMA", "")),
        ("src/PGMB.cbl", program("PGMB", "WORKING-STORAGE", "PGMA", "           MOVE SPACES TO PGMA-X\n")),
        ("src/PGMC.cbl", program("PGMC", "WORKING-STORAGE", "PGMC", "           MOVE SPACES TO PGMC-X\n")),
        ("cpy/PGMA.cpy", "       01  PGMA-COMMAREA.\n           05 PGMA-X PIC X(4).\n".to_owned()),
        ("lib/PGMC.cbl", "       01  PGMC-X PIC X(4).\n".to_owned()),
    ];
    for (name, text) in &files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let at = |p: &str| root.join(p).display().to_string();
    for (source, library) in [("src/PGMA.cbl", "cpy"), ("src/PGMB.cbl", "cpy"), ("src/PGMC.cbl", "lib")] {
        let out = ironwork(&["check", &at(source), "-I", &at(library)]);
        assert!(out.status.success(), "{source}: {}", String::from_utf8_lossy(&out.stderr));
    }
    let _ = std::fs::remove_dir_all(&root);
}
