//! `ironwork fuzz`: a program that adds a quantity from its input file and subscripts with an index
//! from it ends in a data exception and a range check on generated records; each abend is kept once,
//! with a journal that records it, and the same seed finds the same abends on the same inputs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM: &[&str] = &[
    "       CBL SSRANGE",
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. QTYSUM.",
    "       ENVIRONMENT DIVISION.",
    "       INPUT-OUTPUT SECTION.",
    "       FILE-CONTROL.",
    "           SELECT IN-FILE ASSIGN TO INFILE.",
    "       DATA DIVISION.",
    "       FILE SECTION.",
    "       FD IN-FILE.",
    "       01 IN-REC.",
    "          05 IN-NAME PIC X(10).",
    "          05 IN-QTY  PIC 9(5).",
    "          05 IN-IDX  PIC 9(2).",
    "       WORKING-STORAGE SECTION.",
    "       01 WS-TOTAL PIC 9(9) VALUE 0.",
    "       01 WS-EOF PIC X VALUE 'N'.",
    "       01 WS-TABLE.",
    "          05 WS-SLOT PIC X(3) OCCURS 10 TIMES.",
    "       PROCEDURE DIVISION.",
    "           OPEN INPUT IN-FILE",
    "           PERFORM UNTIL WS-EOF = 'Y'",
    "              READ IN-FILE AT END MOVE 'Y' TO WS-EOF",
    "              NOT AT END",
    "                 ADD IN-QTY TO WS-TOTAL",
    "                 MOVE 'ABC' TO WS-SLOT (IN-IDX)",
    "              END-READ",
    "           END-PERFORM",
    "           CLOSE IN-FILE",
    "           GOBACK.",
];

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-fuzz-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("repo/src")).unwrap();
    fs::write(dir.join("repo/src/QTYSUM.cbl"), PROGRAM.join("\n") + "\n").unwrap();
    dir
}

fn fuzz(dir: &Path, out: &str, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .current_dir(dir.join("repo"))
        .args(["fuzz", "src/QTYSUM.cbl", "-o"])
        .arg(dir.join(out))
        .args(extra)
        .output()
        .unwrap()
}

/// Each kept run's abend as `code line` and its inputs' bytes, in the manifest's order.
fn kept(manifest: &str) -> Vec<String> {
    let field = |text: &str, key: &str| text.split(&format!("\"{key}\":")).skip(1).map(|r| r.split([',', '}']).next().unwrap().trim_matches('"').to_string()).collect::<Vec<_>>();
    let runs = manifest.split("\"runs\":[").nth(1).unwrap();
    let codes = field(runs, "code");
    let lines = field(runs, "line");
    let mut out: Vec<String> = codes.iter().zip(&lines).map(|(c, l)| format!("{c} {l}")).collect();
    out.extend(field(manifest.split("\"runs\":[").next().unwrap(), "bytes"));
    out
}

#[test]
fn generated_records_find_the_data_exception_and_the_range_check_each_kept_once_with_its_journal() {
    let dir = temp("find");
    let o = fuzz(&dir, "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let manifest = fs::read_to_string(dir.join("run/manifest.json")).unwrap();
    assert!(manifest.contains("\"tool\":\"ironwork-fuzz\""));
    assert!(manifest.contains("\"program\":{\"file\":\"src/QTYSUM.cbl\",\"id\":\"QTYSUM\"}"));
    assert!(manifest.contains("\"runs\":40"), "{manifest}");
    let found = kept(&manifest);
    assert!(found.iter().any(|k| k == "S0C7 25"), "{found:?}");
    assert!(found.iter().any(|k| k == "U4038 26"), "{found:?}");
    let places: Vec<&String> = found.iter().filter(|k| k.contains(' ')).collect();
    let mut unique = places.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(places.len(), unique.len());
    for journal in manifest.split("\"journal\":\"").skip(1).map(|r| r.split('"').next().unwrap()) {
        let text = fs::read_to_string(dir.join("run/evidence/runs").join(format!("{journal}.jsonl"))).unwrap();
        assert!(text.contains("\"kind\":\"abend\""), "{journal}");
    }
    assert!(dir.join("run/coverage/0.json").exists());
    assert!(!dir.join("run/.work").exists());
}

#[test]
fn the_same_seed_finds_the_same_abends_on_the_same_inputs() {
    let dir = temp("seed");
    for out in ["a", "b"] {
        let o = fuzz(&dir, out, &["--runs", "25", "--seed", "7"]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    let read = |out: &str| kept(&fs::read_to_string(dir.join(out).join("manifest.json")).unwrap());
    assert_eq!(read("a"), read("b"));
}

#[test]
fn a_directory_in_use_or_a_called_program_is_refused() {
    let dir = temp("refuse");
    fs::create_dir_all(dir.join("used")).unwrap();
    fs::write(dir.join("used/keep"), "").unwrap();
    let o = fuzz(&dir, "used", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("is not empty"));

    let called = PROGRAM.iter().map(|l| match *l {
        "       PROCEDURE DIVISION." => "       LINKAGE SECTION.\n       01 LK-X PIC X.\n       PROCEDURE DIVISION USING LK-X.".to_string(),
        l => l.to_string(),
    });
    fs::write(dir.join("repo/src/QTYSUM.cbl"), called.collect::<Vec<_>>().join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "called", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("takes PROCEDURE DIVISION USING"));
}
