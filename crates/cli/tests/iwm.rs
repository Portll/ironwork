//! `ironwork compile` and `ironwork dump` (docs/load-module.md §8 to §12): a module per source or
//! per bundle, the same bytes from any process or directory, nothing written for a program lowering
//! refuses, and a dump that names what is damaged.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-iwm-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

fn cobol(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

/// PAYROLL, with a COPY member from `lib`, an OCCURS DEPENDING ON table and a nested SUB.
fn payroll(dir: &Path) -> PathBuf {
    fs::write(dir.join("lib/CUST.cpy"), cobol(&["01  CUST-REC.", "    05 CUST-ID   PIC 9(5).", "    05 CUST-NAME PIC X(20)."])).unwrap();
    let path = dir.join("PAYROLL.cbl");
    let text = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. PAYROLL.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "COPY CUST.",
        "01  N PIC 9(3) VALUE 5.",
        "01  T.",
        "    05 E PIC X OCCURS 1 TO 10 DEPENDING ON N.",
        "PROCEDURE DIVISION.",
        "MAIN-LINE.",
        "    ADD 1 TO N.",
        "    DISPLAY 'N=' N.",
        "    CALL 'SUB'.",
        "    STOP RUN.",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. SUB.",
        "PROCEDURE DIVISION.",
        "    DISPLAY 'IN SUB'.",
        "    GOBACK.",
        "END PROGRAM SUB.",
        "END PROGRAM PAYROLL.",
    ]);
    fs::write(&path, text).unwrap();
    path
}

fn simple(dir: &Path, id: &str, card: &str, procedure: &[&str]) -> PathBuf {
    let path = dir.join(format!("{id}.cbl"));
    let mut lines = vec![card, "IDENTIFICATION DIVISION."];
    let program_id = format!("PROGRAM-ID. {id}.");
    lines.extend([program_id.as_str(), "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01  W PIC X(21).", "01  B PIC S9(4) COMP VALUE 0.", "PROCEDURE DIVISION."]);
    lines.extend(procedure);
    let text = cobol(&lines).replacen("       \n", "\n", 1);
    fs::write(&path, text).unwrap();
    path
}

/// A program lowering refuses: NUMCHECK beside a compare with ALL ZERO.
fn refused(dir: &Path) -> PathBuf {
    let path = dir.join("MIXED.cbl");
    let text = cobol(&[
        "CBL NUMCHECK",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. MIXED.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  N PIC 9(3) VALUE 5.",
        "PROCEDURE DIVISION.",
        "    IF N = ALL ZERO",
        "        DISPLAY 'Z'",
        "    END-IF.",
        "    STOP RUN.",
    ]);
    fs::write(&path, text).unwrap();
    path
}

fn ironwork(cwd: &Path, args: &[&str], epoch: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    command.current_dir(cwd).args(args);
    match epoch {
        Some(e) => command.env("SOURCE_DATE_EPOCH", e),
        None => command.env_remove("SOURCE_DATE_EPOCH"),
    };
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn compiled(cwd: &Path, args: &[&str], epoch: Option<&str>) {
    let o = ironwork(cwd, &[&["compile"], args].concat(), epoch);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
}

fn dump(module: &Path, args: &[&str]) -> (String, Option<i32>, String) {
    let o = ironwork(Path::new("/"), &[&["dump"], args, &[module.to_str().unwrap()]].concat(), None);
    (text(&o.stdout), o.status.code(), text(&o.stderr))
}

/// Each section's checksum, by name, from the dump's section table.
fn checksums(dump: &str) -> Vec<(String, String)> {
    dump.lines()
        .filter_map(|l| {
            let words: Vec<&str> = l.split(' ').collect();
            (words[0] == "section").then(|| (words[2].to_owned(), words[8].to_owned()))
        })
        .collect()
}

fn differing(a: &str, b: &str) -> Vec<String> {
    checksums(a).into_iter().zip(checksums(b)).filter(|(x, y)| x.1 != y.1).map(|(x, _)| x.0).collect()
}

#[test]
fn a_source_makes_one_module_named_after_it_holding_every_program() {
    let dir = temp("one");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib", "-o", "out"], None);
    let module = dir.join("out/PAYROLL.iwm");
    let bytes = fs::read(&module).unwrap();
    assert!(!text(&bytes).contains(dir.to_str().unwrap()), "the module holds no path");
    let (shown, status, _) = dump(&module, &[]);
    assert_eq!(status, Some(0), "{shown}");
    for line in [
        "format 0.6",
        &format!("length {}", bytes.len()),
        "program 0 PAYROLL parent - common no dynamic yes using [] returning no",
        "program 1 SUB parent 0 common no dynamic yes using [] returning no",
        "PAYROLL item 5 level 05 name E offset 40 size 1 occurs 10 kind Alnum { justified: false } depending on item 3 keys []",
        "PAYROLL source 0 PAYROLL.cbl",
        "PAYROLL source 1 CUST.cpy",
        "PAYROLL file 1 root 1 CUST.cpy sha256 8062b2395984d8d2ec648e2e3a271fee5a4c2ac0b56191eb72147bee17daadc7 bytes 90",
        "PAYROLL #0 CUST.cpy:1:8",
        "program SUB start \"\"",
        "    Display 'IN SUB'",
        "mapsets 0",
        "module reads",
    ] {
        assert!(shown.lines().any(|l| l == line), "{line}\n{shown}");
    }
    let options = [
        "arith", "trunc", "numproc", "codepage", "trunc_check", "fastsrt", "fastsrt_adv_print", "sort_keys", "adv", "thread", "dll", "rent", "dbcs", "warnings", "compile",
        "dynam", "debug", "cics_return_warning", "invdata", "zwb", "quote", "currency", "nsymbol", "dispsign", "intdate", "qualify", "initial", "vlr", "vsamopenfs",
        "dialect",
        "ssrange", "cards", "collating", "decimal_point_comma", "numval_currency", "when_compiled",
    ];
    for field in options {
        assert!(shown.lines().any(|l| l.starts_with(&format!("PAYROLL {field}: "))), "{field}\n{shown}");
    }
    assert!(!shown.contains("STRINGS\n") && !shown.contains("CHECKSUM MISMATCH"));
    let headings: Vec<&str> = shown.lines().filter(|l| ["DIRECTORY", "OPTIONS", "LAYOUT", "LIR", "SQL", "BMS", "DEBUG"].contains(l)).collect();
    assert_eq!(headings, ["DIRECTORY", "OPTIONS", "LAYOUT", "LIR", "SQL", "BMS", "DEBUG"]);
    assert_eq!(dump(&module, &[]).0, shown, "a dump is stable");
}

#[test]
fn a_module_read_and_written_again_is_the_same_bytes() {
    let dir = temp("round");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib", "-o", "."], None);
    let bytes = fs::read(dir.join("PAYROLL.iwm")).unwrap();
    let loaded = exec::module::read(&bytes).unwrap();
    let again = exec::module::write_module(&loaded).unwrap();
    assert_eq!(again, bytes);
    fs::write(dir.join("AGAIN.iwm"), &again).unwrap();
    assert_eq!(dump(&dir.join("AGAIN.iwm"), &[]).0, dump(&dir.join("PAYROLL.iwm"), &[]).0);
}

#[test]
fn two_processes_two_directories_and_a_copy_give_the_same_bytes() {
    let dir = temp("same");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib", "-o", "a"], None);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib", "-o", "b"], None);
    let absolute = dir.join("c");
    compiled(Path::new("/"), &[dir.join("PAYROLL.cbl").to_str().unwrap(), "-I", dir.join("lib").to_str().unwrap(), "-o", absolute.to_str().unwrap()], None);
    let copy = dir.join("elsewhere/src");
    fs::create_dir_all(copy.join("copylib")).unwrap();
    fs::copy(dir.join("PAYROLL.cbl"), copy.join("PAYROLL.cbl")).unwrap();
    fs::copy(dir.join("lib/CUST.cpy"), copy.join("copylib/CUST.cpy")).unwrap();
    compiled(&dir.join("elsewhere"), &["src/PAYROLL.cbl", "-I", "./src/copylib/", "-o", "../d"], None);
    let first = fs::read(dir.join("a/PAYROLL.iwm")).unwrap();
    for other in ["b", "c", "d"] {
        assert!(fs::read(dir.join(other).join("PAYROLL.iwm")).unwrap() == first, "{other}");
    }
}

#[test]
fn when_compiled_is_source_date_epoch_s_time_or_the_clock_s_and_changes_only_options() {
    let dir = temp("when");
    simple(&dir, "STAMP", "", &["    MOVE FUNCTION WHEN-COMPILED TO W.", "    GOBACK."]);
    let module = |out: &str| dir.join(out).join("STAMP.iwm");
    compiled(&dir, &["STAMP.cbl", "-o", "e1"], Some("315532800"));
    compiled(&dir, &["STAMP.cbl", "-o", "e2"], Some("315532800"));
    assert!(fs::read(module("e1")).unwrap() == fs::read(module("e2")).unwrap());
    let (epoch, _, _) = dump(&module("e1"), &["--section", "OPTIONS"]);
    assert!(epoch.contains("STAMP when_compiled: Some(CompileTime { seconds: 315532800, hundredths: 0, source: SourceDateEpoch })"), "{epoch}");

    compiled(&dir, &["STAMP.cbl", "-o", "c1"], None);
    std::thread::sleep(std::time::Duration::from_millis(30));
    compiled(&dir, &["STAMP.cbl", "-o", "c2"], None);
    let (one, two) = (dump(&module("c1"), &[]).0, dump(&module("c2"), &[]).0);
    assert!(one.contains("source: Clock })"), "{one}");
    assert_eq!(differing(&one, &two), ["OPTIONS"]);

    simple(&dir, "PLAIN", "", &["    MOVE FUNCTION CURRENT-DATE TO W.", "    GOBACK."]);
    compiled(&dir, &["PLAIN.cbl", "-o", "p1"], None);
    std::thread::sleep(std::time::Duration::from_millis(30));
    compiled(&dir, &["PLAIN.cbl", "-o", "p2"], None);
    assert!(fs::read(dir.join("p1/PLAIN.iwm")).unwrap() == fs::read(dir.join("p2/PLAIN.iwm")).unwrap());
    assert!(dump(&dir.join("p1/PLAIN.iwm"), &[]).0.contains("PLAIN when_compiled: None"));
}

#[test]
fn an_option_changes_the_options_its_card_s_text_and_what_it_changes_in_the_lir() {
    let dir = temp("option");
    let procedure = ["    ADD 9999 TO B.", "    GOBACK."];
    simple(&dir, "OPT", "CBL TRUNC(STD)", &procedure);
    compiled(&dir, &["OPT.cbl", "-o", "std"], None);
    simple(&dir, "OPT", "CBL TRUNC(BIN)", &procedure);
    compiled(&dir, &["OPT.cbl", "-o", "bin"], None);
    let (std, bin) = (dump(&dir.join("std/OPT.iwm"), &[]).0, dump(&dir.join("bin/OPT.iwm"), &[]).0);
    assert!(bin.contains("OPT trunc: Bin") && std.contains("OPT trunc: Std"));
    let changed = differing(&std, &bin);
    assert!(changed.contains(&"OPTIONS".to_owned()));
    assert!(changed.iter().all(|s| ["STRINGS", "OPTIONS", "LIR", "DEBUG"].contains(&s.as_str())), "{changed:?}");
    assert!(std.lines().zip(bin.lines()).filter(|(a, b)| a != b && a.starts_with("OPT file ")).count() == 1, "only the source's digest differs in DEBUG");
}

#[test]
fn a_construct_lowering_refuses_is_named_where_it_is_and_nothing_is_written_for_its_source() {
    let dir = temp("refused");
    refused(&dir);
    payroll(&dir);
    let o = ironwork(&dir, &["compile", "MIXED.cbl", "PAYROLL.cbl", "-I", "lib", "-o", "out"], None);
    assert_eq!(o.status.code(), Some(12));
    let err = text(&o.stderr);
    assert!(err.contains("MIXED.cbl:8:12: lowering: NUMCHECK with ALL ZERO or ALL NULL compared with a data item it may test is not lowered yet"), "{err}");
    assert!(err.contains("ironwork: MIXED.iwm not written"), "{err}");
    assert!(!dir.join("out/MIXED.iwm").exists());
    assert!(dir.join("out/PAYROLL.iwm").exists());
    assert!(fs::read_dir(dir.join("out")).unwrap().count() == 1, "no partial file is left");
}

#[test]
fn a_bundle_holds_every_source_s_programs_in_one_directory_or_is_not_written() {
    let dir = temp("bundle");
    payroll(&dir);
    simple(&dir, "OTHER", "", &["    GOBACK."]);
    compiled(&dir, &["PAYROLL.cbl", "OTHER.cbl", "-I", "lib", "-o", "out", "--bundle", "x"], None);
    assert!(!dir.join("out/PAYROLL.iwm").exists());
    let (shown, status, _) = dump(&dir.join("out/x.iwm"), &["--section", "DIRECTORY", "--section", "DEBUG"]);
    assert_eq!(status, Some(0));
    for line in ["program 1 SUB parent 0 common no dynamic yes using [] returning no", "program 2 OTHER parent - common no dynamic yes using [] returning no", "OTHER source 0 OTHER.cbl", "PAYROLL source 0 PAYROLL.cbl"] {
        assert!(shown.lines().any(|l| l == line), "{line}\n{shown}");
    }
    assert!(!shown.contains("\nLIR\n") && !shown.contains("\nOPTIONS\n"));

    refused(&dir);
    let o = ironwork(&dir, &["compile", "PAYROLL.cbl", "MIXED.cbl", "-I", "lib", "-o", "out2", "--bundle", "y"], None);
    assert_eq!(o.status.code(), Some(12));
    assert!(!dir.join("out2/y.iwm").exists());
}

#[test]
fn source_prefix_names_the_source_in_the_debug_table() {
    let dir = temp("prefix");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib", "--source-prefix", "app/batch"], None);
    let (shown, _, _) = dump(&dir.join("PAYROLL.iwm"), &["--section", "DEBUG"]);
    assert!(shown.contains("PAYROLL source 0 app/batch/PAYROLL.cbl\nPAYROLL source 1 CUST.cpy\n"), "{shown}");
    let o = ironwork(&dir, &["compile", "PAYROLL.cbl", "-I", "lib", "--source-prefix", "../up"], None);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
}

#[test]
fn the_sql_table_has_one_entry_per_block_with_dense_ordinals() {
    let dir = temp("sql");
    let text = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. SQLP.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "    EXEC SQL INCLUDE SQLCA END-EXEC.",
        "01  ID-VAL PIC S9(9) COMP.",
        "    EXEC SQL DECLARE C1 CURSOR WITH HOLD FOR",
        "        SELECT ID FROM T WHERE ID > :ID-VAL",
        "    END-EXEC.",
        "PROCEDURE DIVISION.",
        "    EXEC SQL WHENEVER SQLERROR GO TO FAILED END-EXEC.",
        "    EXEC SQL INSERT INTO T (ID) VALUES (:ID-VAL) END-EXEC.",
        "    EXEC SQL OPEN C1 END-EXEC.",
        "    STOP RUN.",
        "FAILED.",
        "    STOP RUN.",
    ]);
    fs::write(dir.join("SQLP.cbl"), text).unwrap();
    compiled(&dir, &["SQLP.cbl"], None);
    let (shown, status, _) = dump(&dir.join("SQLP.iwm"), &["--section", "SQL"]);
    assert_eq!(status, Some(0));
    let identities: Vec<&str> = shown.lines().filter(|l| l.starts_with("SQLP:") && !l.contains(" statement ") && !l.ends_with(" with hold")).collect();
    let fingerprint = |t: &str| format!("{:08x}", t.bytes().fold(0x811C_9DC5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193)));
    assert_eq!(identities[2], format!("SQLP:3:{} INSERT INTO T (ID) VALUES (?)", fingerprint("INSERT INTO T (ID) VALUES (?)")));
    let ordinals: Vec<&str> = identities.iter().map(|l| l.split(':').nth(1).unwrap()).collect();
    assert_eq!(ordinals, ["1", "2", "3", "4"]);
    assert!(shown.contains(&format!("SQLP:4:{} with hold", fingerprint("DECLARE C1 CURSOR WITH HOLD FOR SELECT ID FROM T WHERE ID > ?"))), "{shown}");
}

#[test]
fn dump_prints_a_bad_section_as_a_checksum_mismatch_and_the_rest() {
    let dir = temp("damaged");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib"], None);
    let module = dir.join("PAYROLL.iwm");
    let (shown, _, _) = dump(&module, &[]);
    let layout = shown.lines().find(|l| l.starts_with("section 4 LAYOUT")).unwrap();
    let offset: usize = layout.split(' ').nth(4).unwrap().parse().unwrap();
    let mut bytes = fs::read(&module).unwrap();
    bytes[offset + 3] ^= 0x01;
    fs::write(&module, &bytes).unwrap();
    let (damaged, status, _) = dump(&module, &[]);
    assert_eq!(status, Some(1));
    assert!(damaged.lines().any(|l| l.starts_with("section 4 LAYOUT ") && l.contains("CHECKSUM MISMATCH")), "{damaged}");
    assert!(damaged.contains("LAYOUT not printed: CHECKSUM MISMATCH"));
    assert!(damaged.contains("PAYROLL #0 CUST.cpy:1:8") && damaged.contains("program 1 SUB parent 0"));
    assert!(damaged.contains("module refused: section LAYOUT is corrupt (checksum "));
    let (unchecked, _, _) = dump(&module, &["--no-check"]);
    assert!(!unchecked.contains("not printed") && unchecked.contains("CHECKSUM MISMATCH"));
}

#[test]
fn dump_refuses_what_is_not_a_module_by_its_own_check() {
    let dir = temp("refuse");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib"], None);
    let good = fs::read(dir.join("PAYROLL.iwm")).unwrap();
    let refused = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        let (out, status, err) = dump(&path, &[]);
        assert_eq!((out.as_str(), status), ("", Some(1)));
        err
    };
    assert!(refused("source.iwm", b"       IDENTIFICATION DIVISION.").ends_with("source.iwm: not an ironwork load module\n"));
    assert!(refused("short.iwm", &good[..20]).ends_with("short.iwm: truncated: 20 bytes of 32\n"));
    assert!(refused("cut.iwm", &good[..100]).ends_with(&format!("cut.iwm: truncated: 100 bytes of {}\n", good.len())));
    let mut major = good.clone();
    major[8..12].copy_from_slice(&[1, 0, 0, 0]);
    assert!(refused("major.iwm", &major).ends_with("major.iwm: load module format 1.0; this ironwork reads 0.5 to 0.6. Compile the source again\n"));
    let mut minor = good.clone();
    minor[10] = 1;
    assert!(refused("minor.iwm", &minor).contains("load module format 0.1;"));
    let mut feature = good.clone();
    feature[12] = 4;
    let count = u32::from_le_bytes(feature[16..20].try_into().unwrap()) as usize;
    let crc = exec::module::crc::extend(exec::module::crc::crc32(&feature[..28]), &feature[32..32 + count * 28]);
    feature[28..32].copy_from_slice(&crc.to_le_bytes());
    assert!(refused("feature.iwm", &feature).ends_with("feature.iwm: load module needs features 0x00000004, which this ironwork lacks\n"));
    let mut table = good;
    table[40] ^= 1;
    assert!(refused("table.iwm", &table).contains("header is corrupt (checksum "));
}

/// The LIR section of a dump: the listing of each program, without the section table or the reader's verdict.
fn listing(dump: &str) -> &str {
    let start = dump.find("\nLIR\n").map_or(dump.len(), |at| at + "\nLIR\n".len());
    let end = dump.rfind("\nmodule ").map_or(dump.len(), |at| at + 1).max(start);
    &dump[start..end]
}

/// Each program of tests/lir compiled and its code printed (lir.md §13), against NAME.lir there.
/// IRONWORK_BLESS=1 writes what is printed as the expected text.
#[test]
fn each_golden_program_prints_its_generated_code_as_expected() {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lir");
    let dir = temp("golden");
    let mut sources: Vec<PathBuf> = fs::read_dir(&golden).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "cbl")).collect();
    sources.sort();
    assert!(sources.len() >= 6, "{sources:?}");
    let bless = std::env::var_os("IRONWORK_BLESS").is_some();
    let mut differing = Vec::new();
    for source in &sources {
        let name = source.file_stem().unwrap().to_str().unwrap();
        compiled(&golden, &[&format!("{name}.cbl"), "-o", dir.to_str().unwrap()], None);
        let (shown, status, _) = dump(&dir.join(format!("{name}.iwm")), &["--section", "LIR"]);
        assert_eq!(status, Some(0), "{shown}");
        let printed = listing(&shown);
        let expected = golden.join(format!("{name}.lir"));
        if bless {
            fs::write(&expected, printed).unwrap();
        } else if fs::read_to_string(&expected).ok().as_deref() != Some(printed) {
            differing.push(format!("{name}:\n{printed}"));
        }
        assert_eq!(listing(&dump(&dir.join(format!("{name}.iwm")), &["--section", "LIR"]).0), printed, "{name} prints the same twice");
    }
    assert!(differing.is_empty(), "printed otherwise than tests/lir/NAME.lir holds (IRONWORK_BLESS=1 rewrites it):\n{}", differing.join("\n"));
}

#[test]
fn the_listing_names_what_the_other_sections_hold_and_prints_without_them() {
    let dir = temp("listing");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lir");
    compiled(&golden, &["SQLWHEN.cbl", "-o", dir.to_str().unwrap()], None);
    let module = dir.join("SQLWHEN.iwm");
    let whole = dump(&module, &["--section", "LIR"]).0;
    assert!(whole.contains("    Sql 2 \"SELECT NAME FROM CUST WHERE ID = ?\" query inputs (CUST-ID [integer]) into (CUST-NAME [char(20)])\n"), "{whole}");
    assert!(whole.contains("    statement 11:12\n") && whole.contains("    Display 'SQLCODE ', SQLCODE [digits 9 signed]\n"), "{whole}");

    let (shown, _, _) = dump(&module, &[]);
    let mut bytes = fs::read(&module).unwrap();
    for section in ["section 6 SQL ", "section 8 DEBUG "] {
        let offset: usize = shown.lines().find(|l| l.starts_with(section)).unwrap().split(' ').nth(4).unwrap().parse().unwrap();
        bytes[offset + 1] ^= 0x01;
    }
    fs::write(&module, &bytes).unwrap();
    let (damaged, status, _) = dump(&module, &["--section", "LIR"]);
    assert_eq!(status, Some(1));
    let printed = listing(&damaged);
    assert!(printed.contains("    Sql 2\n") && !printed.contains("statement ") && !printed.contains('@'), "{printed}");
    assert!(printed.contains("    Display 'SQLCODE ', SQLCODE [digits 9 signed]\n"), "{printed}");
}

#[test]
fn dump_shows_the_string_table_only_when_asked() {
    let dir = temp("strings");
    payroll(&dir);
    compiled(&dir, &["PAYROLL.cbl", "-I", "lib"], None);
    let (all, _, _) = dump(&dir.join("PAYROLL.iwm"), &["--strings"]);
    assert!(all.contains("STRINGS\nstring 0 \"PAYROLL\"\nstring 1 \"SUB\"\n"), "{all}");
    let (only, _, _) = dump(&dir.join("PAYROLL.iwm"), &["--section", "strings"]);
    assert!(only.contains("string 0 \"PAYROLL\"") && !only.contains("\nDIRECTORY\n"));
}

#[test]
fn the_flags_of_compile_and_dump_are_theirs_alone() {
    let dir = temp("usage");
    payroll(&dir);
    let o = ironwork(&dir, &["run", "PAYROLL.cbl", "-o", "out"], None);
    assert_eq!(o.status.code(), Some(246), "run's usage: {}", text(&o.stderr));
    for args in [
        &["check", "PAYROLL.cbl", "--strings"][..],
        &["compile"],
        &["compile", "PAYROLL.cbl", "--dd", "X=y"],
        &["compile", "PAYROLL.cbl", "--no-check"],
        &["compile", "PAYROLL.cbl", "--bundle", "a/b"],
        &["compile", "PAYROLL.cbl", "sub/PAYROLL.cbl"],
        &["dump", "PAYROLL.iwm", "-I", "lib"],
        &["dump", "--section", "NOPE", "PAYROLL.iwm"],
        &["dump"],
    ] {
        let o = ironwork(&dir, args, None);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o.stderr));
    }
    assert!(!dir.join("PAYROLL.iwm").exists());
}
