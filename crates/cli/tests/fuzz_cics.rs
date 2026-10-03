//! `ironwork fuzz --cics`: an order program that adds a count from its COMMAREA and a quantity
//! typed into its map ends in ASRA on a generated COMMAREA and on generated operator input; each
//! abend is kept once with a journal that records it and the input that gave it, and the
//! pseudo-conversation runs as the transaction RETURN TRANSID names.

mod schema;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use schema::read_manifest;

const PROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. ORDCICS.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "           COPY ORDSET.",
    "       01 WS-RESP PIC S9(8) COMP.",
    "       01 WS-TOTAL PIC 9(7) VALUE 0.",
    "       01 WS-COMM.",
    "          05 WS-STATE PIC X VALUE 'S'.",
    "          05 WS-COUNT PIC 9(3) VALUE 0.",
    "       LINKAGE SECTION.",
    "       01 DFHCOMMAREA.",
    "          05 CA-STATE PIC X.",
    "          05 CA-COUNT PIC 9(3).",
    "       PROCEDURE DIVISION.",
    "           IF EIBCALEN = 0",
    "              MOVE LOW-VALUES TO ORDMAPO",
    "              EXEC CICS SEND MAP('ORDMAP') MAPSET('ORDSET') ERASE",
    "              END-EXEC",
    "              EXEC CICS RETURN TRANSID('ORD1') COMMAREA(WS-COMM)",
    "              END-EXEC",
    "           END-IF",
    "           ADD CA-COUNT TO WS-TOTAL",
    "           EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET')",
    "                RESP(WS-RESP) END-EXEC",
    "           IF WS-RESP = DFHRESP(NORMAL)",
    "              COMPUTE WS-TOTAL = WS-TOTAL + QTYI",
    "           END-IF",
    "           EXEC CICS RETURN TRANSID('ORD1') COMMAREA(WS-COMM)",
    "           END-EXEC.",
];

fn bms() -> String {
    let card = |text: &str, continued: bool| if continued { format!("{text:<71}X\n") } else { format!("{text}\n") };
    [
        card("ORDSET   DFHMSD TYPE=&SYSPARM,MODE=INOUT,LANG=COBOL,STORAGE=AUTO,", true),
        card("               CTRL=(FREEKB,FRSET)", false),
        card("ORDMAP   DFHMDI SIZE=(24,80),LINE=1,COLUMN=1", false),
        card("         DFHMDF POS=(3,1),LENGTH=9,ATTRB=ASKIP,INITIAL='CUSTOMER:'", false),
        card("CUST     DFHMDF POS=(3,11),LENGTH=8,ATTRB=(UNPROT,IC)", false),
        card("         DFHMDF POS=(3,20),LENGTH=1,ATTRB=ASKIP", false),
        card("QTY      DFHMDF POS=(4,11),LENGTH=3,ATTRB=(UNPROT,NUM),PICIN='999'", false),
        card("         DFHMDF POS=(4,15),LENGTH=1,ATTRB=ASKIP", false),
        card("         DFHMSD TYPE=FINAL", false),
        card("         END", false),
    ]
    .concat()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-fuzz-cics-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("repo/src")).unwrap();
    fs::write(dir.join("repo/src/ORDCICS.cbl"), PROGRAM.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/ORDSET.bms"), bms()).unwrap();
    dir
}

fn fuzz(dir: &Path, program: &str, out: &str, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["fuzz", "--cics", program, "-o"]).arg(dir.join(out)).args(extra).output().unwrap()
}

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Each kept run's abend as `code line`, in the manifest's order.
fn kept(manifest: &str) -> Vec<String> {
    let field = |text: &str, key: &str| text.split(&format!("\"{key}\":")).skip(1).map(|r| r.split([',', '}']).next().unwrap().trim_matches('"').to_string()).collect::<Vec<_>>();
    let runs = manifest.split("\"runs\":[").nth(1).unwrap();
    field(runs, "code").iter().zip(field(runs, "line")).map(|(c, l)| format!("{c} {l}")).collect()
}

/// RFC 4648 base64, as the manifest carries an input's bytes.
fn unbase64(text: &str) -> Vec<u8> {
    let value = |c: u8| b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().position(|&a| a == c).unwrap() as u32;
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let n = chunk.iter().filter(|&&c| c != b'=').enumerate().fold(0, |n, (i, &c)| n | value(c) << (18 - 6 * i));
        out.extend([(n >> 16) as u8, (n >> 8) as u8, n as u8].into_iter().take(chunk.iter().filter(|&&c| c != b'=').count() - 1));
    }
    out
}

#[test]
fn a_generated_commarea_and_typed_quantity_each_end_in_asra_kept_once_with_its_journal() {
    let dir = temp("find");
    let o = fuzz(&dir, "src/ORDCICS.cbl", "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("each task runs as transaction ORD1"), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"entry\":\"cics\""));
    assert!(manifest.contains("\"program\":{\"file\":\"src/ORDCICS.cbl\",\"id\":\"ORDCICS\"}"));
    let found = kept(&manifest);
    assert!(found.iter().any(|k| k == "ASRA 23"), "{found:?}");
    assert!(found.iter().any(|k| k == "ASRA 27"), "{found:?}");
    let mut unique = found.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(found.len(), unique.len());
    assert!(manifest.contains("\"kind\":\"commarea\""), "{manifest}");
    let script = manifest
        .split("\"kind\":\"terminal\"")
        .next()
        .and_then(|before| before.rsplit("\"bytes\":\"").next())
        .map(|b| String::from_utf8(unbase64(b.split('"').next().unwrap())).unwrap())
        .unwrap();
    assert!(script.contains("home\ntab\nstring ") && script.lines().last().is_some_and(|l| !l.starts_with("string")), "{script}");
    assert!(manifest.contains("\"name\":\"TERM\""));
    assert!(manifest.contains("S0C7, which CICS reports as ASRA"));
    for journal in manifest.split("\"journal\":\"").skip(1).map(|r| r.split('"').next().unwrap()) {
        let text = fs::read_to_string(dir.join("run/evidence/runs").join(format!("{journal}.jsonl"))).unwrap();
        assert!(text.contains("\"kind\":\"abend\""), "{journal}");
    }
    let coverage = fs::read_to_string(dir.join("run/coverage/0.json")).unwrap();
    assert!(coverage.contains("\"program\":\"ORDCICS\""), "{coverage}");
    assert!(!dir.join("run/.work").exists());
}

#[test]
fn the_same_seed_finds_the_same_abends_on_the_same_inputs() {
    let dir = temp("seed");
    for out in ["a", "b"] {
        let o = fuzz(&dir, "src/ORDCICS.cbl", out, &["--runs", "20", "--seed", "11"]);
        assert!(o.status.success(), "{}", stderr(&o));
    }
    let read = |out: &str| read_manifest(&dir.join(out));
    assert_eq!(kept(&read("a")), kept(&read("b")));
    let inputs = |m: String| m.split("\"inputs\":").nth(1).unwrap().split("\"program\":").next().unwrap().to_string();
    assert_eq!(inputs(read("a")), inputs(read("b")));
}

#[test]
fn a_program_with_no_commarea_and_no_terminal_input_has_nothing_to_vary() {
    let dir = temp("nothing");
    let quiet = ["       IDENTIFICATION DIVISION.", "       PROGRAM-ID. QUIET.", "       PROCEDURE DIVISION.", "           EXEC CICS RETURN END-EXEC."];
    fs::write(dir.join("repo/src/QUIET.cbl"), quiet.join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "src/QUIET.cbl", "run", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("QUIET declares no DFHCOMMAREA and reads no terminal"), "{}", stderr(&o));
}

#[test]
fn a_key_a_task_never_receives_ends_with_that_task() {
    let dir = temp("unread");
    let keys = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. KEYS.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "           COPY DFHAID.",
        "       01 WS-IN PIC X(20).",
        "       01 WS-C PIC X VALUE 'C'.",
        "       PROCEDURE DIVISION.",
        "           IF EIBCALEN = 0",
        "              EXEC CICS RETURN TRANSID('KEY1') COMMAREA(WS-C) END-EXEC",
        "           END-IF",
        "           IF EIBAID NOT = DFHENTER",
        "              DISPLAY 'OTHER KEY'",
        "              EXEC CICS RETURN TRANSID('KEY1') COMMAREA(WS-C) END-EXEC",
        "           END-IF",
        "           EXEC CICS RECEIVE INTO(WS-IN) END-EXEC",
        "           DISPLAY 'ENTER'",
        "           EXEC CICS RETURN END-EXEC.",
    ];
    fs::write(dir.join("repo/src/KEYS.cbl"), keys.join("\n") + "\n").unwrap();
    fs::write(dir.join("screens"), "PF5\nENTER\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .current_dir(dir.join("repo"))
        .args(["cics", "src/KEYS.cbl", "--transid", "KEY1", "--screens"])
        .arg(dir.join("screens"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let began = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if began.elapsed() > std::time::Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("the conversation did not end: PF5 started a task again and again");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "OTHER KEY\nENTER\n");
    assert!(stderr(&o).contains("task 3: KEY1 runs KEYS") && !stderr(&o).contains("task 4"), "{}", stderr(&o));
}

#[test]
fn a_task_writes_its_coverage_across_the_pseudo_conversation() {
    let dir = temp("coverage");
    fs::write(dir.join("screens"), "home\nstring ACME\ntab\nstring 7\nENTER\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .current_dir(dir.join("repo"))
        .args(["cics", "src/ORDCICS.cbl", "--transid", "ORD1", "--screens"])
        .arg(dir.join("screens"))
        .arg("--coverage")
        .arg(dir.join("coverage.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("task 2: ORD1 runs ORDCICS"), "{}", stderr(&o));
    let coverage = fs::read_to_string(dir.join("coverage.json")).unwrap();
    assert!(coverage.contains("\"program\":\"ORDCICS\""), "{coverage}");
}
