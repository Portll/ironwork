//! `ironwork run x.iwm` and `ironwork cics x.iwm` (docs/load-module.md §8): modules `ironwork
//! compile` writes run on the VM and give the output and status the interpreter gives running their
//! source, with CALL, CANCEL, INVOKE, a user-defined function and SEND MAP reaching programs,
//! classes and mapsets through the loader, and with the coverage report and evidence journal the
//! source's run gives; a module the loader cannot load stops the run and says why.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-iwm-run-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

fn cobol(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

fn write(dir: &Path, file: &str, lines: &[&str]) {
    let path = dir.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, cobol(lines)).unwrap();
}

fn ironwork(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(cwd).args(args).env_remove("SOURCE_DATE_EPOCH").output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn compiled(cwd: &Path, args: &[&str]) {
    let o = ironwork(cwd, &[&["compile"], args].concat());
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
}

/// Standard output and exit status, which a module's run and its source's agree on.
fn ran(o: &Output) -> (String, Option<i32>) {
    (text(&o.stdout), o.status.code())
}

/// The ABEND lines of standard error.
fn abends(o: &Output) -> Vec<String> {
    text(&o.stderr).lines().filter(|l| l.contains(": ABEND ")).map(str::to_owned).collect()
}

const DOUBLE: &[&str] = &[
    "IDENTIFICATION DIVISION.",
    "FUNCTION-ID. DOUBLE AS 'dbl'.",
    "DATA DIVISION.",
    "LINKAGE SECTION.",
    "01  N PIC 9(3).",
    "01  R PIC 9(4).",
    "PROCEDURE DIVISION USING N RETURNING R.",
    "    COMPUTE R = N * 2",
    "    GOBACK.",
    "END FUNCTION DOUBLE.",
];

/// MAIN calls SUB statically, then dynamically, cancels it, calls it again, invokes a function
/// defined in its source by its external name, and calls a program that is nowhere.
fn main_program(dir: &Path) {
    let mut lines = DOUBLE.to_vec();
    lines.extend([
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. MAIN.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  NAME PIC X(8) VALUE 'SUB'.",
        "01  K PIC 9(3) VALUE 21.",
        "PROCEDURE DIVISION.",
        "    CALL 'SUB'",
        "    CALL 'SUB'",
        "    CALL NAME",
        "    CANCEL NAME",
        "    CALL NAME",
        "    DISPLAY 'DOUBLE ' FUNCTION DOUBLE(K)",
        "    CALL 'NOSUCH' ON EXCEPTION DISPLAY 'NO NOSUCH' END-CALL",
        "    MOVE 3 TO RETURN-CODE",
        "    GOBACK.",
        "END PROGRAM MAIN.",
    ]);
    write(dir, "MAIN.cbl", &lines);
}

/// SUB counts its CALLs and calls SUBIN, which it contains and which counts its own, so a CANCEL
/// of SUB that reaches SUBIN starts both counts again.
fn sub_program(dir: &Path, file: &str, shown: &str) {
    let display = format!("    DISPLAY '{shown} ' COUNTER");
    write(
        dir,
        file,
        &[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. SUB.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  COUNTER PIC 9(3) VALUE 0.",
            "PROCEDURE DIVISION.",
            "    ADD 1 TO COUNTER",
            &display,
            "    CALL 'SUBIN'",
            "    GOBACK.",
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. SUBIN.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  INNER PIC 9(3) VALUE 0.",
            "PROCEDURE DIVISION.",
            "    ADD 1 TO INNER",
            "    DISPLAY 'SUBIN ' INNER",
            "    GOBACK.",
            "END PROGRAM SUBIN.",
            "END PROGRAM SUB.",
        ],
    );
}

const EXPECTED: &str = "SUB 001\nSUBIN 001\nSUB 002\nSUBIN 002\nSUB 003\nSUBIN 003\nSUB 001\nSUBIN 001\nDOUBLE 0042\nNO NOSUCH\n";

#[test]
fn static_and_dynamic_call_cancel_and_a_function_run_as_the_source_runs() {
    let dir = temp("call");
    main_program(&dir);
    sub_program(&dir, "lib/SUB.cbl", "SUB");
    let source = ironwork(&dir, &["run", "MAIN.cbl", "-L", "lib"]);
    assert_eq!(ran(&source), (EXPECTED.to_owned(), Some(3)), "{}", text(&source.stderr));

    compiled(&dir, &["MAIN.cbl", "-o", "app"]);
    compiled(&dir, &["lib/SUB.cbl", "-o", "mods"]);
    let shown = text(&ironwork(&dir, &["dump", "--section", "DIRECTORY", "app/MAIN.iwm"]).stdout);
    assert!(shown.contains("program 1 DOUBLE parent - common no dynamic yes using [reference] returning yes\nprogram 1 external dbl\n"), "{shown}");
    let module = ironwork(&dir, &["run", "app/MAIN.iwm", "-L", "mods"]);
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));
    assert_eq!(text(&module.stderr), "");

    fs::copy(dir.join("mods/SUB.iwm"), dir.join("app/SUB.iwm")).unwrap();
    let own_directory = ironwork(&dir.join("app"), &["run", "MAIN.iwm"]);
    assert_eq!(ran(&own_directory), ran(&source), "{}", text(&own_directory.stderr));

    let vm = ironwork(&dir, &["run", "--vm", "MAIN.cbl", "-L", "mods"]);
    assert_eq!(ran(&vm), ran(&source), "a source run on the VM finds its callee as a module: {}", text(&vm.stderr));
}

#[test]
fn a_bundle_resolves_its_calls_within_itself_first() {
    let dir = temp("bundle");
    main_program(&dir);
    sub_program(&dir, "lib/SUB.cbl", "SUB");
    sub_program(&dir, "decoy/SUB.cbl", "DECOY");
    let source = ironwork(&dir, &["run", "MAIN.cbl", "-L", "lib"]);
    compiled(&dir, &["MAIN.cbl", "lib/SUB.cbl", "-o", "out", "--bundle", "app"]);
    compiled(&dir, &["decoy/SUB.cbl", "-o", "out"]);
    let module = ironwork(&dir, &["run", "out/app.iwm"]);
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));
    assert!(!text(&module.stdout).contains("DECOY"));
}

#[test]
fn a_class_is_loaded_from_its_module_for_invoke() {
    let dir = temp("class");
    write(
        &dir,
        "Account.cbl",
        &[
            "CBL THREAD,DLL",
            "IDENTIFICATION DIVISION.",
            "CLASS-ID. Account INHERITS Base.",
            "ENVIRONMENT DIVISION.",
            "CONFIGURATION SECTION.",
            "REPOSITORY.",
            "    CLASS Base IS \"java.lang.Object\"",
            "    CLASS Account IS \"Account\".",
            "IDENTIFICATION DIVISION.",
            "FACTORY.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  OPENED PIC S9(9) BINARY VALUE 0.",
            "PROCEDURE DIVISION.",
            "IDENTIFICATION DIVISION.",
            "METHOD-ID. \"open\".",
            "DATA DIVISION.",
            "LINKAGE SECTION.",
            "01  OPENING USAGE OBJECT REFERENCE Account.",
            "PROCEDURE DIVISION RETURNING OPENING.",
            "    INVOKE Account NEW RETURNING OPENING",
            "    ADD 1 TO OPENED.",
            "END METHOD \"open\".",
            "END FACTORY.",
            "IDENTIFICATION DIVISION.",
            "OBJECT.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  BALANCE PIC S9(9) BINARY VALUE 100.",
            "PROCEDURE DIVISION.",
            "IDENTIFICATION DIVISION.",
            "METHOD-ID. \"credit\".",
            "DATA DIVISION.",
            "LINKAGE SECTION.",
            "01  AMOUNT PIC S9(9) BINARY.",
            "PROCEDURE DIVISION USING BY VALUE AMOUNT.",
            "    ADD AMOUNT TO BALANCE.",
            "END METHOD \"credit\".",
            "IDENTIFICATION DIVISION.",
            "METHOD-ID. \"getBalance\".",
            "DATA DIVISION.",
            "LINKAGE SECTION.",
            "01  RESULT PIC S9(9) BINARY.",
            "PROCEDURE DIVISION RETURNING RESULT.",
            "    MOVE BALANCE TO RESULT.",
            "END METHOD \"getBalance\".",
            "END OBJECT.",
            "END CLASS Account.",
        ],
    );
    write(
        &dir,
        "CLIENT.cbl",
        &[
            "CBL THREAD,DLL",
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. CLIENT RECURSIVE.",
            "ENVIRONMENT DIVISION.",
            "CONFIGURATION SECTION.",
            "REPOSITORY.",
            "    CLASS Account IS \"Account\".",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  A1 USAGE OBJECT REFERENCE Account.",
            "01  AMOUNT PIC S9(9) BINARY.",
            "01  BAL PIC S9(9) BINARY.",
            "01  SHOWN PIC ZZZ9.",
            "PROCEDURE DIVISION.",
            "    INVOKE Account \"open\" RETURNING A1",
            "    MOVE 25 TO AMOUNT",
            "    INVOKE A1 \"credit\" USING BY VALUE AMOUNT",
            "    INVOKE A1 \"getBalance\" RETURNING BAL",
            "    MOVE BAL TO SHOWN DISPLAY 'A1 ' SHOWN",
            "    GOBACK.",
        ],
    );
    let source = ironwork(&dir, &["run", "CLIENT.cbl"]);
    assert_eq!(ran(&source), ("A1  125\n".to_owned(), Some(0)), "{}", text(&source.stderr));
    compiled(&dir, &["CLIENT.cbl", "Account.cbl", "-o", "out"]);
    let module = ironwork(&dir, &["run", "out/CLIENT.iwm"]);
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));
}

/// MISS calls a program that is nowhere, first with ON EXCEPTION and then without.
fn missing(dir: &Path) {
    write(
        dir,
        "MISS.cbl",
        &[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. MISS.",
            "PROCEDURE DIVISION.",
            "    CALL 'NOSUCH' ON EXCEPTION DISPLAY 'ON EXCEPTION' END-CALL",
            "    DISPLAY 'AGAIN'",
            "    CALL 'NOSUCH'",
            "    DISPLAY 'NOT REACHED'",
            "    GOBACK.",
        ],
    );
}

#[test]
fn a_program_that_is_nowhere_runs_on_exception_or_abends_s806_as_the_source_does() {
    let dir = temp("missing");
    missing(&dir);
    let source = ironwork(&dir, &["run", "MISS.cbl"]);
    assert_eq!(ran(&source), ("ON EXCEPTION\nAGAIN\n".to_owned(), Some(240)));
    compiled(&dir, &["MISS.cbl", "-o", "out"]);
    let module = ironwork(&dir, &["run", "out/MISS.iwm"]);
    assert_eq!(ran(&module), ran(&source));
    assert_eq!(abends(&module), abends(&source));
    assert!(abends(&module)[0].starts_with("MISS.cbl:6:12: ABEND S806: CALL NOSUCH: "), "{:?}", abends(&module));
}

#[test]
fn a_module_holding_no_program_of_the_name_is_a_load_error_that_on_exception_does_not_catch() {
    let dir = temp("load-error");
    missing(&dir);
    compiled(&dir, &["MISS.cbl", "-o", "out"]);
    fs::copy(dir.join("out/MISS.iwm"), dir.join("out/NOSUCH.iwm")).unwrap();
    let module = ironwork(&dir, &["run", "out/MISS.iwm"]);
    assert_eq!(ran(&module), (String::new(), Some(244)));
    assert_eq!(abends(&module), ["MISS.cbl:4:12: ABEND IRONWORK: CALL NOSUCH: out/NOSUCH.iwm: the module holds no program NOSUCH"]);
}

/// A copy of `module` with one byte of section `name` changed, its checksum left as it was.
fn damaged(module: &Path, name: &str) -> Vec<u8> {
    let shown = text(&ironwork(Path::new("/"), &["dump", "--section", "STRINGS", module.to_str().unwrap()]).stdout);
    let line = shown.lines().find(|l| l.split(' ').nth(2) == Some(name)).unwrap();
    let offset: usize = line.split(' ').nth(4).unwrap().parse().unwrap();
    let mut bytes = fs::read(module).unwrap();
    bytes[offset] ^= 0x01;
    bytes
}

#[test]
fn a_damaged_module_is_refused_whether_called_or_run() {
    let dir = temp("damaged");
    main_program(&dir);
    sub_program(&dir, "lib/SUB.cbl", "SUB");
    compiled(&dir, &["MAIN.cbl", "lib/SUB.cbl", "-o", "out"]);
    let sub = dir.join("out/SUB.iwm");
    fs::write(&sub, damaged(&sub, "LIR")).unwrap();
    let called = ironwork(&dir, &["run", "out/MAIN.iwm"]);
    assert_eq!(ran(&called), (String::new(), Some(244)));
    let line = &abends(&called)[0];
    assert!(line.starts_with("MAIN.cbl:18:12: ABEND IRONWORK: CALL SUB: out/SUB.iwm: section LIR is corrupt (checksum "), "{line}");

    let main = dir.join("out/MAIN.iwm");
    fs::write(dir.join("out/BAD.iwm"), damaged(&main, "LAYOUT")).unwrap();
    let run = ironwork(&dir, &["run", "out/BAD.iwm"]);
    assert_eq!(ran(&run), (String::new(), Some(245)));
    assert!(text(&run.stderr).starts_with("ironwork: out/BAD.iwm: section LAYOUT is corrupt (checksum "), "{}", text(&run.stderr));

    let mut old = fs::read(&main).unwrap();
    old[10] = 1;
    fs::write(dir.join("out/OLD.iwm"), old).unwrap();
    let run = ironwork(&dir, &["run", "out/OLD.iwm"]);
    assert_eq!((ran(&run), text(&run.stderr)), ((String::new(), Some(245)), "ironwork: out/OLD.iwm: load module format 0.1; this ironwork reads 0.5 to 0.7. Compile the source again\n".to_owned()));
}

#[test]
fn a_module_beside_its_source_in_a_program_library_is_the_one_the_vm_runs() {
    let dir = temp("shadow");
    main_program(&dir);
    sub_program(&dir, "lib/SUB.cbl", "SUB");
    compiled(&dir, &["lib/SUB.cbl", "-o", "lib"]);
    sub_program(&dir, "lib/SUB.cbl", "NEWER");
    let vm = ironwork(&dir, &["run", "--vm", "MAIN.cbl", "-L", "lib"]);
    assert_eq!(ran(&vm), (EXPECTED.to_owned(), Some(3)), "{}", text(&vm.stderr));
    let interpreted = ironwork(&dir, &["run", "MAIN.cbl", "-L", "lib"]);
    assert!(text(&interpreted.stdout).starts_with("NEWER 001\n"), "the interpreter reads source only");
}

#[test]
fn a_main_program_s_abend_parm_and_return_code_are_the_source_s() {
    let dir = temp("parm");
    write(
        &dir,
        "PARMS.cbl",
        &[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. PARMS.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  STARS PIC X(3) VALUE '***'.",
            "01  BAD REDEFINES STARS PIC 9(3).",
            "01  GOOD PIC 9(3) VALUE 1.",
            "LINKAGE SECTION.",
            "01  PARM.",
            "    05 PARM-LEN PIC S9(4) COMP.",
            "    05 PARM-TEXT PIC X(10).",
            "PROCEDURE DIVISION USING PARM.",
            "    DISPLAY 'PARM ' PARM-LEN ' ' PARM-TEXT(1:PARM-LEN)",
            "    ADD BAD TO GOOD",
            "    GOBACK.",
        ],
    );
    compiled(&dir, &["PARMS.cbl", "-o", "out"]);
    let source = ironwork(&dir, &["run", "PARMS.cbl", "--parm", "HELLO"]);
    let module = ironwork(&dir, &["run", "out/PARMS.iwm", "--parm", "HELLO"]);
    assert!(text(&source.stdout).starts_with("PARM 0005 HELLO\n"), "{}", text(&source.stdout));
    assert_eq!(ran(&module), ran(&source));
    assert_eq!(abends(&module), abends(&source));
    assert!(abends(&module)[0].starts_with("PARMS.cbl:14:16: ABEND S0C7: "), "{:?}", abends(&module));
}

#[test]
fn a_map_a_module_s_program_sends_comes_from_the_module() {
    let dir = temp("bms");
    let card = |line: &str, continued: bool| if continued { format!("{line:<71}X\n") } else { format!("{line}\n") };
    let bms = [
        card("ORDSET   DFHMSD TYPE=&SYSPARM,MODE=INOUT,LANG=COBOL,STORAGE=AUTO,", true),
        card("               CTRL=(FREEKB,FRSET)", false),
        card("ORDMAP   DFHMDI SIZE=(24,80),LINE=1,COLUMN=1", false),
        card("         DFHMDF POS=(3,1),LENGTH=9,ATTRB=ASKIP,INITIAL='CUSTOMER:'", false),
        card("CUST     DFHMDF POS=(3,11),LENGTH=8,ATTRB=(UNPROT,IC)", false),
        card("         DFHMDF POS=(3,20),LENGTH=1,ATTRB=ASKIP", false),
        card("         DFHMSD TYPE=FINAL", false),
        card("         END", false),
    ];
    fs::write(dir.join("lib/ORDSET.bms"), bms.concat()).unwrap();
    let sender = |id: &str, mapset: &str| {
        let program_id = format!("PROGRAM-ID. {id}.");
        let send = format!("    EXEC CICS SEND MAP('ORDMAP') MAPSET('{mapset}') FROM(ORDMAPO)");
        let lines = [
            "IDENTIFICATION DIVISION.",
            &program_id,
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "    COPY ORDSET.",
            "PROCEDURE DIVISION.",
            "    MOVE LOW-VALUES TO ORDMAPO",
            "    MOVE 'ACME' TO CUSTO",
            &send,
            "         ERASE END-EXEC",
            "    EXEC CICS RETURN END-EXEC.",
        ];
        write(&dir, &format!("lib/{id}.cbl"), &lines);
    };
    sender("MAPPGM", "ORDSET");
    write(
        &dir,
        "front/FRONT.cbl",
        &["IDENTIFICATION DIVISION.", "PROGRAM-ID. FRONT.", "PROCEDURE DIVISION.", "    EXEC CICS LINK PROGRAM('MAPPGM') END-EXEC", "    DISPLAY 'BACK IN FRONT'", "    EXEC CICS RETURN END-EXEC."],
    );
    fs::write(dir.join("screens"), "").unwrap();
    let source = ironwork(&dir, &["cics", "front/FRONT.cbl", "-L", "lib", "-I", "lib", "--screens", "screens"]);
    assert!(text(&source.stdout).contains("BACK IN FRONT\n--- screen 1 ---\n\n\n CUSTOMER: ACME"), "{}", text(&source.stderr));
    compiled(&dir, &["lib/MAPPGM.cbl", "-o", "out"]);
    let shown = text(&ironwork(&dir, &["dump", "--section", "BMS", "out/MAPPGM.iwm"]).stdout);
    assert!(shown.contains("mapsets 1\nmapset ORDSET mode InOut ctrl [FREEKB FRSET] maps 1\nmapset ORDSET map ORDMAP size 24x80 at 1,1"), "{shown}");
    let module = ironwork(&dir, &["cics", "--vm", "front/FRONT.cbl", "-L", "out", "--screens", "screens"]);
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));

    sender("MAPTWO", "ordset");
    compiled(&dir, &["lib/MAPPGM.cbl", "lib/MAPTWO.cbl", "-o", "out", "--bundle", "maps"]);
    let both = text(&ironwork(&dir, &["dump", "--section", "BMS", "out/maps.iwm"]).stdout);
    assert_eq!((both.matches("\nmapsets 1\n").count(), both.matches("\nmapset ORDSET mode").count()), (1, 1), "{both}");

    fs::write(dir.join("lib/BADSET.bms"), "BADSET   DFHMSD TYPE=&SYSPARM,MODE=INOUT\n         DFHMDF POS=(\n").unwrap();
    sender("BADPGM", "BADSET");
    let refused = ironwork(&dir, &["compile", "lib/BADPGM.cbl", "-o", "out"]);
    assert_eq!(refused.status.code(), Some(12));
    assert!(text(&refused.stderr).contains("lib/BADPGM.cbl: mapset BADSET: "), "{}", text(&refused.stderr));
    assert!(!dir.join("out/BADPGM.iwm").exists());
}

#[test]
fn a_module_runs_with_run_and_cics_and_refuses_what_only_a_source_takes() {
    let dir = temp("usage");
    missing(&dir);
    compiled(&dir, &["MISS.cbl", "-o", "out"]);
    let refused = [
        (&["check", "out/MISS.iwm"][..], "check takes a source", 2),
        (&["run", "out/MISS.iwm", "-silent"], "the compile flags", 246),
        (&["cics", "out/MISS.iwm", "--optimize=2"], "the compile flags", 246),
        (&["run", "out/MISS.iwm", "--provenance", "p.json"], "--provenance", 246),
        (&["cics", "out/MISS.iwm", "--serve", "127.0.0.1:0"], "--serve", 246),
        (&["cics", "out/MISS.iwm", "--serve-public"], "--serve-public", 246),
        (&["run", "out/MISS.iwm", "--transid", "MISS"], "the cics flags are for cics", 246),
    ];
    for (args, named, usage) in refused {
        let o = ironwork(&dir, args);
        assert_eq!(o.status.code(), Some(usage), "{args:?}: {}", text(&o.stderr));
        let first = text(&o.stderr).lines().next().unwrap_or_default().to_owned();
        assert!(first.contains("out/MISS.iwm is a load module") && first.contains(named), "{args:?}: {first}");
    }
    let o = ironwork(&dir, &["compile", "out/MISS.iwm", "-o", "again"]);
    assert_eq!((o.status.code(), text(&o.stderr)), (Some(16), "ironwork: out/MISS.iwm is a load module; compile takes a source\nironwork: MISS.iwm not written\n".to_owned()));
}

/// FIRSTP shows what started its task, reads a CICS file, writes a transient-data queue, LINKs to
/// HELPER and returns TRANSID NEXT with a COMMAREA. LIBPGM, which NEXT runs, divides by zero.
fn conversation(dir: &Path) {
    write(
        dir,
        "src/FIRSTP.cbl",
        &[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. FIRSTP.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  WS-AREA PIC X(10) VALUE 'FROMFIRST '.",
            "01  WS-KEY PIC X(5) VALUE '00002'.",
            "01  WS-REC PIC X(15).",
            "01  WS-USER PIC X(8).",
            "01  WS-APPL PIC X(8).",
            "01  WS-SYS PIC X(4).",
            "LINKAGE SECTION.",
            "01  DFHCOMMAREA PIC X(10).",
            "PROCEDURE DIVISION.",
            "MAIN-PARA.",
            "    IF EIBCALEN > 0",
            "        DISPLAY 'GOT ' DFHCOMMAREA",
            "    END-IF",
            "    EXEC CICS ASSIGN USERID(WS-USER) APPLID(WS-APPL)",
            "        SYSID(WS-SYS) END-EXEC",
            "    DISPLAY EIBTRNID ' ' EIBTRMID ' ' WS-USER ' ' WS-APPL",
            "        ' ' WS-SYS",
            "    EXEC CICS READ FILE('CUSTF') INTO(WS-REC)",
            "        RIDFLD(WS-KEY) END-EXEC",
            "    DISPLAY 'READ ' WS-REC",
            "    EXEC CICS WRITEQ TD QUEUE('LOGQ') FROM(WS-AREA)",
            "        LENGTH(10) END-EXEC",
            "    EXEC CICS LINK PROGRAM('HELPER') END-EXEC",
            "    PERFORM SHOW",
            "    EXEC CICS RETURN TRANSID('NEXT') COMMAREA(WS-AREA)",
            "        END-EXEC.",
            "SHOW.",
            "    DISPLAY 'FIRST DONE'.",
        ],
    );
    write(dir, "lib/HELPER.cbl", &["IDENTIFICATION DIVISION.", "PROGRAM-ID. HELPER.", "PROCEDURE DIVISION.", "H1.", "    DISPLAY 'HELPER'", "    EXEC CICS RETURN END-EXEC."]);
    write(
        dir,
        "lib/LIBPGM.cbl",
        &[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. LIBPGM.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  D PIC 9 VALUE 0.",
            "01  Q PIC 9.",
            "LINKAGE SECTION.",
            "01  DFHCOMMAREA PIC X(10).",
            "PROCEDURE DIVISION.",
            "L1.",
            "    DISPLAY 'NEXT GOT ' DFHCOMMAREA ' ' EIBTRNID",
            "    DIVIDE 10 BY D GIVING Q",
            "    EXEC CICS RETURN END-EXEC.",
        ],
    );
    fs::write(dir.join("custf.txt"), "00001ALICE     \n00002BOB       \n").unwrap();
    fs::write(dir.join("comm"), "HELLO     \n").unwrap();
    fs::write(dir.join("screens"), "ENTER\n").unwrap();
    fs::write(dir.join("next.csd"), " DEFINE TRANSACTION(NEXT) GROUP(G) PROGRAM(LIBPGM)\n").unwrap();
    compiled(dir, &["src/FIRSTP.cbl", "-o", "src"]);
    compiled(dir, &["lib/HELPER.cbl", "lib/LIBPGM.cbl", "-o", "mods"]);
}

/// A `cics` run of `program` with every flag a task takes, its files and queue named after `tag`.
fn task(dir: &Path, program: &str, tag: &str, extra: &[&str]) -> Output {
    fs::copy(dir.join("custf.txt"), dir.join(format!("custf-{tag}.txt"))).unwrap();
    let file = format!("CUSTF=custf-{tag}.txt,KSDS,key=0:5,len=15,text");
    let (td, out) = (format!("LOGQ=td-{tag}.txt"), format!("comm-{tag}:text"));
    let identity = ["--transid", "FIRS", "--termid", "T001", "--userid", "ADA", "--applid", "APPL1", "--sysid", "SYS1"];
    let args = [&["cics", program][..], &identity, &["--file", &file, "--td", &td, "--commarea", "comm:text", "--commarea-out", &out], extra].concat();
    ironwork(dir, &args)
}

#[test]
fn a_cics_task_from_a_module_runs_as_the_source_s_task_runs() {
    let dir = temp("cics");
    conversation(&dir);
    let source = task(&dir, "src/FIRSTP.cbl", "cbl", &["-L", "lib"]);
    assert_eq!(ran(&source), ("GOT HELLO     \nFIRS T001 ADA      APPL1    SYS1\nREAD 00002BOB       \nHELPER\nFIRST DONE\n".to_owned(), Some(0)), "{}", text(&source.stderr));
    assert!(text(&source.stderr).ends_with("ironwork: RETURN TRANSID(NEXT) with a 10-byte COMMAREA\n"), "{}", text(&source.stderr));
    for lib in ["mods", "lib"] {
        let tag = format!("iwm-{lib}");
        let module = task(&dir, "src/FIRSTP.iwm", &tag, &["-L", lib]);
        assert_eq!(ran(&module), ran(&source), "-L {lib}: {}", text(&module.stderr));
        assert_eq!(text(&module.stderr), "ironwork: RETURN TRANSID(NEXT) with a 10-byte COMMAREA\n");
        for kept in ["comm", "td", "custf"] {
            let (made, given) = (fs::read(dir.join(format!("{kept}-{tag}{}", if kept == "comm" { "" } else { ".txt" }))), fs::read(dir.join(format!("{kept}-cbl{}", if kept == "comm" { "" } else { ".txt" }))));
            assert_eq!(made.unwrap(), given.unwrap(), "{kept}, -L {lib}");
        }
    }

    for next in [&["--transaction", "NEXT=LIBPGM"][..], &["--csd", "next.csd"]] {
        let conversed = |program: &str, tag: &str, lib: &str| task(&dir, program, tag, &[&["-L", lib, "--screens", "screens"][..], next].concat());
        let source = conversed("src/FIRSTP.cbl", "cbl", "lib");
        assert!(text(&source.stdout).contains("FIRST DONE\nNEXT GOT FROMFIRST  NEXT\n"), "{}", text(&source.stderr));
        assert_eq!(source.status.code(), Some(240));
        for lib in ["mods", "lib"] {
            let module = conversed("src/FIRSTP.iwm", &format!("iwm-{lib}"), lib);
            assert_eq!(ran(&module), ran(&source), "{next:?} -L {lib}: {}", text(&module.stderr));
            let (from_module, from_source) = (abends(&module), abends(&source));
            assert_eq!(from_module.len(), 1, "{}", text(&module.stderr));
            assert!(from_source[0].ends_with(&from_module[0]), "{from_source:?} {from_module:?}");
            assert!(from_module[0].contains("LIBPGM.cbl:12:12: ABEND ASRA: "), "{from_module:?}");
            assert!(text(&module.stderr).contains("ironwork: task 2: NEXT runs LIBPGM\n"));
        }
    }
}

/// A journal's records, without what each run's journal has of its own: when each record was
/// written, the chain and hashes that link them, and how long the run took.
fn journal(dir: &Path) -> Vec<String> {
    let runs: Vec<PathBuf> = fs::read_dir(dir.join("runs")).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let own = |line: &str| {
        let mut kept = line.to_owned();
        for key in ["at", "chain", "hash", "prev", "durationMs"] {
            let needle = format!("\"{key}\":");
            if let Some(start) = kept.find(&needle) {
                let end = kept[start..].find([',', '}']).map_or(kept.len(), |e| start + e + usize::from(kept[start + e..].starts_with(',')));
                kept.replace_range(start..end, "");
            }
        }
        kept
    };
    fs::read_to_string(&runs[0]).unwrap().lines().map(own).collect()
}

/// The module run's journal is the source run's, but for the file its command line names.
fn same_journal(module: &[String], source: &[String], iwm: &str, cbl: &str) {
    let (iwm, cbl) = (format!("\"{iwm}\"]"), format!("\"{cbl}\"]"));
    assert!(module[0].contains("\"kind\":\"open\"") && module[0].contains(&iwm), "{}", module[0]);
    assert_eq!(module[0].replace(&iwm, &cbl), source[0]);
    assert_eq!(module[1..], source[1..]);
}

#[test]
fn a_module_run_writes_the_coverage_and_journal_its_source_s_run_writes() {
    let dir = temp("evidence");
    write(
        &dir,
        "src/EVD.cbl",
        &[
            "IDENTIFICATION DIVISION.",
            "FUNCTION-ID. DOUBLE AS 'dbl'.",
            "DATA DIVISION.",
            "LINKAGE SECTION.",
            "01  N PIC 9(3).",
            "01  R PIC 9(4).",
            "PROCEDURE DIVISION USING N RETURNING R.",
            "CALC.",
            "    COMPUTE R = N * 2",
            "    GOBACK.",
            "END FUNCTION DOUBLE.",
            "IDENTIFICATION DIVISION.",
            "FUNCTION-ID. TRIPLE AS 'trp' IS PROTOTYPE.",
            "DATA DIVISION.",
            "LINKAGE SECTION.",
            "01  N PIC 9(3).",
            "01  R PIC 9(4).",
            "PROCEDURE DIVISION USING N RETURNING R.",
            "END FUNCTION TRIPLE.",
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. EVD.",
            "ENVIRONMENT DIVISION.",
            "INPUT-OUTPUT SECTION.",
            "FILE-CONTROL.",
            "    SELECT IN-FILE ASSIGN TO INFILE.",
            "    SELECT OUT-FILE ASSIGN TO OUTFILE.",
            "DATA DIVISION.",
            "FILE SECTION.",
            "FD IN-FILE.",
            "    COPY INREC.",
            "FD OUT-FILE.",
            "01 OUT-REC PIC X(10).",
            "WORKING-STORAGE SECTION.",
            "    COPY BADNUM.",
            "01 NAME PIC X(8) VALUE 'HELPER'.",
            "01 K PIC 9(3) VALUE 7.",
            "PROCEDURE DIVISION.",
            "FIRST-PART SECTION.",
            "OPENING.",
            "    OPEN INPUT IN-FILE OUTPUT OUT-FILE",
            "    READ IN-FILE END-READ",
            "    MOVE IN-REC TO OUT-REC",
            "    DISPLAY 'REC ' IN-REC",
            "    PERFORM TWICE 2 TIMES",
            "    CALL NAME",
            "    CALL 'INNER'",
            "    DISPLAY FUNCTION DOUBLE(K) ' ' FUNCTION TRIPLE(K)",
            "    WRITE OUT-REC",
            "    CLOSE IN-FILE OUT-FILE.",
            "    COPY ADDBAD.",
            "    GOBACK.",
            "TWICE.",
            "    DISPLAY 'TWICE'.",
            "NEVER.",
            "    DISPLAY 'NEVER'.",
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. INNER.",
            "PROCEDURE DIVISION.",
            "    DISPLAY 'INNER'",
            "    GOBACK.",
            "END PROGRAM INNER.",
            "END PROGRAM EVD.",
        ],
    );
    write(&dir, "copy/sys/INREC.cpy", &["01 IN-REC PIC X(10)."]);
    write(&dir, "copy/BADNUM.cpy", &["01 WS-A PIC X(3) VALUE '***'.", "01 WS-N REDEFINES WS-A PIC 9(3).", "01 WS-T PIC 9(3) VALUE 0."]);
    write(&dir, "copy/ADDBAD.cpy", &["    ADD WS-N TO WS-T."]);
    write(
        &dir,
        "lib/HELPER.cbl",
        &["IDENTIFICATION DIVISION.", "PROGRAM-ID. HELPER.", "PROCEDURE DIVISION.", "    DISPLAY 'HELPER'", "    CALL 'HELPIN'", "    GOBACK.", "IDENTIFICATION DIVISION.", "PROGRAM-ID. HELPIN.", "PROCEDURE DIVISION.", "    DISPLAY 'HELPIN'", "    GOBACK.", "END PROGRAM HELPIN.", "END PROGRAM HELPER."],
    );
    write(
        &dir,
        "lib/TRP.cbl",
        &["IDENTIFICATION DIVISION.", "FUNCTION-ID. TRIPLE AS 'trp'.", "DATA DIVISION.", "LINKAGE SECTION.", "01  N PIC 9(3).", "01  R PIC 9(4).", "PROCEDURE DIVISION USING N RETURNING R.", "    COMPUTE R = N * 3", "    GOBACK.", "END FUNCTION TRIPLE."],
    );
    fs::write(dir.join("in.txt"), "HELLOWORLD\n").unwrap();
    fs::write(dir.join("statements"), "EVD.cbl:43\nADDBAD.cpy:1\nHELPER.cbl:4\n").unwrap();
    compiled(&dir, &["src/EVD.cbl", "-I", "copy", "-I", "copy/sys", "-o", "src"]);
    compiled(&dir, &["lib/HELPER.cbl", "lib/TRP.cbl", "-o", "lib"]);
    let run = |program: &str, tag: &str| {
        let output = format!("OUTFILE=out-{tag}.txt:text");
        let (coverage, evidence) = (format!("coverage-{tag}.json"), format!("ev-{tag}"));
        let flags = ["--trace-statements", "statements", "--trace-marker", "HELPER", "--trace-input"];
        ironwork(&dir, &[&["run", program, "-I", "copy", "-I", "copy/sys", "-L", "lib", "--dd", "INFILE=in.txt:text", "--dd", &output, "--coverage", &coverage, "--evidence", &evidence][..], &flags].concat())
    };
    let source = run("src/EVD.cbl", "cbl");
    assert_eq!(ran(&source), ("REC HELLOWORLD\nTWICE\nTWICE\nHELPER\nHELPIN\nINNER\n0014 0021\n".to_owned(), Some(240)), "{}", text(&source.stderr));
    let module = run("src/EVD.iwm", "iwm");
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));

    let coverage = fs::read_to_string(dir.join("coverage-cbl.json")).unwrap();
    assert_eq!(fs::read_to_string(dir.join("coverage-iwm.json")).unwrap(), coverage);
    for part in [
        "{\"entered\":2,\"line\":52,\"name\":\"TWICE\",\"section\":false}",
        "{\"entered\":0,\"line\":54,\"name\":\"NEVER\",\"section\":false}",
        "\"called\":[{\"program\":\"HELPER\",\"reached\":[\"\"]},{\"program\":\"HELPIN\",\"reached\":[\"\"]},{\"program\":\"TRIPLE\",\"reached\":[\"\"]}]",
    ] {
        assert!(coverage.contains(part), "{part}\n{coverage}");
    }

    let (from_module, from_source) = (journal(&dir.join("ev-iwm")), journal(&dir.join("ev-cbl")));
    same_journal(&from_module, &from_source, "EVD.iwm", "EVD.cbl");
    for kind in ["\"kind\":\"input\",\"path\":\"INREC.cpy\",\"root\":2", "\"from\":\"HELPER.cbl\",\"kind\":\"call\",\"program\":\"HELPIN\"", "\"from\":\"TRP.cbl\",\"kind\":\"call\",\"program\":\"TRP\"", "\"file\":\"ADDBAD.cpy\",\"kind\":\"statement\"", "\"kind\":\"sink\"", "\"code\":\"S0C7\",\"file\":\"ADDBAD.cpy\",\"kind\":\"abend\",\"line\":1"] {
        assert!(from_module.iter().any(|r| r.contains(kind)), "{kind}\n{from_module:#?}");
    }
}

#[test]
fn a_module_s_cics_tasks_write_the_coverage_and_journal_the_source_s_write() {
    let dir = temp("cics-evidence");
    conversation(&dir);
    compiled(&dir, &["lib/HELPER.cbl", "lib/LIBPGM.cbl", "-o", "lib"]);
    let run = |program: &str, tag: &str, lib: &str| {
        let (coverage, evidence) = (format!("coverage-{tag}.json"), format!("ev-{tag}"));
        task(&dir, program, tag, &["-L", lib, "--screens", "screens", "--transaction", "NEXT=LIBPGM", "--coverage", &coverage, "--evidence", &evidence, "--trace-input", "--trace-marker", "HELLO"])
    };
    let source = run("src/FIRSTP.cbl", "cbl", "lib");
    assert_eq!(source.status.code(), Some(240), "{}", text(&source.stderr));
    let module = run("src/FIRSTP.iwm", "iwm", "lib");
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));
    let coverage = fs::read_to_string(dir.join("coverage-cbl.json")).unwrap();
    assert_eq!(fs::read_to_string(dir.join("coverage-iwm.json")).unwrap(), coverage);
    assert!(coverage.contains("{\"program\":\"HELPER\",\"reached\":[\"H1\"]},{\"program\":\"LIBPGM\",\"reached\":[\"L1\"]}"), "{coverage}");
    let (from_module, from_source) = (journal(&dir.join("ev-iwm")), journal(&dir.join("ev-cbl")));
    same_journal(&from_module, &from_source, "FIRSTP.iwm", "FIRSTP.cbl");
    for kind in ["\"from\":\"HELPER.cbl\",\"kind\":\"call\",\"program\":\"HELPER\"", "\"code\":\"ASRA\",\"file\":\"LIBPGM.cbl\",\"kind\":\"abend\",\"line\":12", "\"marker\":\"HELLO\""] {
        assert!(from_module.iter().any(|r| r.contains(kind)), "{kind}\n{from_module:#?}");
    }
}

#[test]
fn a_recording_answers_a_module_s_exec_sql_as_it_answers_the_source_s() {
    let dir = temp("sql");
    write(
        &dir,
        "SQLQ.cbl",
        &[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. SQLQ.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "    EXEC SQL INCLUDE SQLCA END-EXEC.",
            "01  WS-NAME PIC X(8).",
            "01  WS-CODE PIC -9(3).",
            "PROCEDURE DIVISION.",
            "    EXEC SQL DECLARE C1 CURSOR FOR",
            "             SELECT NAME FROM SERVED ORDER BY ID END-EXEC.",
            "    EXEC SQL OPEN C1 END-EXEC.",
            "    EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.",
            "    MOVE SQLCODE TO WS-CODE.",
            "    DISPLAY 'FIRST ' WS-NAME WS-CODE.",
            "    EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.",
            "    MOVE SQLCODE TO WS-CODE.",
            "    DISPLAY 'THEN ' WS-CODE.",
            "    GOBACK.",
        ],
    );
    let fingerprint = |t: &str| t.bytes().fold(0x811C_9DC5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    let (declare, fetch, commit) = (fingerprint("DECLARE C1 CURSOR FOR SELECT NAME FROM SERVED ORDER BY ID"), fingerprint("FETCH C1"), fingerprint("COMMIT"));
    let recording = format!(
        "# ironwork sql recording 1\n@ 1 SQLQ:2:{declare:08x} OPEN C1\n< 0 00000 rows=0\n@ 2 SQLQ:3:{fetch:08x} FETCH C1\n< 0 00000 rows=0\n= char:\"ADAMS\"\n@ 3 SQLQ:4:{fetch:08x} FETCH C1\n< 100 02000 rows=0\n@ 4 SQLQ:0:{commit:08x} COMMIT\n< 0 00000 rows=0\n"
    );
    fs::write(dir.join("rec.sql"), recording).unwrap();
    let source = ironwork(&dir, &["run", "SQLQ.cbl", "--sql-replay", "rec.sql"]);
    assert_eq!(ran(&source), ("FIRST ADAMS    000\nTHEN  100\n".to_owned(), Some(0)), "{}", text(&source.stderr));
    compiled(&dir, &["SQLQ.cbl", "-o", "out"]);
    let module = ironwork(&dir, &["run", "out/SQLQ.iwm", "--sql-replay", "rec.sql"]);
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));
}

#[test]
fn a_dynamic_callee_in_the_module_gets_the_storage_the_interpreter_gives_it() {
    let dir = temp("address");
    let program = |id: &str, size: u8, calls: &[&str]| {
        let mut lines = vec![
            "IDENTIFICATION DIVISION.".to_owned(),
            format!("PROGRAM-ID. {id}."),
            "DATA DIVISION.".into(),
            "WORKING-STORAGE SECTION.".into(),
            format!("01  X PIC X({size})."),
            "01  P USAGE POINTER.".into(),
            "01  N REDEFINES P PIC S9(9) COMP-5.".into(),
            "PROCEDURE DIVISION.".into(),
            "    SET P TO ADDRESS OF X".into(),
            format!("    DISPLAY '{id} ' N"),
        ];
        lines.extend(calls.iter().map(|c| format!("    CALL '{c}'")));
        lines.extend(["    GOBACK.".to_owned(), format!("END PROGRAM {id}.")]);
        lines
    };
    let mut lines = vec!["CBL DYNAM".to_owned()];
    lines.extend(program("A", 10, &["B", "B"]));
    lines.extend(program("B", 3, &[]));
    write(&dir, "A.cbl", &lines.iter().map(String::as_str).collect::<Vec<_>>());
    let source = ironwork(&dir, &["run", "A.cbl"]);
    assert_eq!(text(&source.stdout).lines().count(), 3, "{}", text(&source.stderr));
    compiled(&dir, &["A.cbl", "-o", "out"]);
    let module = ironwork(&dir, &["run", "out/A.iwm"]);
    assert_eq!(ran(&module), ran(&source), "{}", text(&module.stderr));
}
