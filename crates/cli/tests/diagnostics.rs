use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

/// A source in the temp directory, removed when dropped.
struct Source(PathBuf);

impl Source {
    fn new(name: &str, text: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ironwork-diagnostics-{}-{name}.cbl", std::process::id()));
        std::fs::write(&path, text).unwrap();
        Source(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// An object-oriented program with no CBL card, so without THREAD and DLL; `data` and `body` add
/// to its WORKING-STORAGE and PROCEDURE DIVISION.
fn object_oriented(data: &str, body: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CLIENT.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  A1 USAGE OBJECT REFERENCE Account.\n{data}       PROCEDURE DIVISION.\n{body}           IF A1 = NULL DISPLAY 'NO ACCOUNT' END-IF\n           GOBACK.\n"
    )
}

const MISSING: &str = "warning: program CLIENT uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: THREAD, DLL missing from its CBL or PROCESS cards (see J13 and J19)";

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// What run and cics add to the compile's messages when the compile gives no program to run.
fn not_run(path: &str, return_code: u8) -> String {
    format!("ironwork: {path}: the program does not run: the compile's return code is {return_code}\n")
}

#[test]
fn check_exits_0_with_nothing_to_say() {
    let source = Source::new("clean", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           GOBACK.\n");
    let out = ironwork(&["check", source.path()]);
    assert_eq!((out.status.code(), stderr(&out)), (Some(0), String::new()));
}

#[test]
fn an_object_oriented_program_without_thread_and_dll_checks_with_return_code_4_and_runs() {
    let source = Source::new("oo", &object_oriented("", ""));
    let warning = format!("{}: {MISSING}\n", source.path());
    let checked = ironwork(&["check", source.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()));
    let ran = ironwork(&["run", source.path()]);
    assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning.clone(), "NO ACCOUNT\n".into()));
    let task = ironwork(&["cics", source.path()]);
    assert_eq!(task.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&task.stdout), "NO ACCOUNT\n");
    assert!(stderr(&task).starts_with(&warning) && stderr(&task).contains("ironwork: the task ended"), "{}", stderr(&task));
}

#[test]
fn warnings_block_refuses_the_run_and_keeps_return_code_4() {
    let source = Source::new("blocked", &object_oriented("", ""));
    let warning = format!("{}: {MISSING}\n", source.path());
    let refused = warning.clone() + &not_run(source.path(), 4);
    for (command, status, said) in [("check", 4, &warning), ("run", 241, &refused), ("cics", 241, &refused)] {
        let out = ironwork(&[command, source.path(), "-warnings-block"]);
        assert_eq!((out.status.code(), stderr(&out), out.stdout.is_empty()), (Some(status), said.clone(), true), "{command}");
    }
}

#[test]
fn an_error_keeps_its_line_and_return_code_12_and_is_listed_before_warnings() {
    let refused = Source::new("undefined", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  X PIC X.\n       PROCEDURE DIVISION.\n           MOVE Y TO X.\n           GOBACK.\n");
    let error = format!("{}:7:17: IWC0001-S Y is not defined\n", refused.path());
    for (command, status, said) in [("check", 12, error.clone()), ("run", 241, error.clone() + &not_run(refused.path(), 12))] {
        let out = ironwork(&[command, refused.path()]);
        assert_eq!((out.status.code(), stderr(&out), out.stdout.is_empty()), (Some(status), said, true), "{command}");
    }
    let mixed = Source::new("mixed", &object_oriented("       01  X PIC X.\n", "           MOVE Y TO X\n"));
    let out = ironwork(&["check", mixed.path()]);
    assert_eq!(out.status.code(), Some(12));
    assert_eq!(stderr(&out), format!("{0}:12:17: IWC0001-S Y is not defined\n{0}: {MISSING}\n", mixed.path()));
}

#[test]
fn a_syntax_error_is_return_code_12() {
    let source = Source::new("syntax", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           MOVE TO.\n");
    let out = ironwork(&["check", source.path()]);
    assert_eq!((out.status.code(), stderr(&out)), (Some(12), format!("{}:4:19: IWS0001-S expected TO, found Period\n", source.path())));
}

/// `object_oriented`'s program, which compiles with a warning, behind a CBL card.
fn carded(card: &str) -> String {
    format!("       CBL {card}\n{}", object_oriented("", ""))
}

#[test]
fn a_nocompile_card_says_where_run_refuses_and_outranks_warnings_block() {
    let blocked = Source::new("nocompile-w", &carded("NOCOMPILE(W)"));
    let warning = format!("{}: {MISSING}\n", blocked.path());
    let refused = warning.clone() + &not_run(blocked.path(), 4);
    for (command, status, said) in [("check", 4, &warning), ("run", 241, &refused), ("cics", 241, &refused)] {
        let out = ironwork(&[command, blocked.path()]);
        assert_eq!((out.status.code(), stderr(&out), out.stdout.is_empty()), (Some(status), said.clone(), true), "{command}");
    }
    for (k, card) in ["NOC(E)", "NOCOMPILE(S)", "COMPILE"].into_iter().enumerate() {
        let source = Source::new(&format!("card-{k}"), &carded(card));
        let warning = format!("{}: {MISSING}\n", source.path());
        let checked = ironwork(&["check", source.path(), "-warnings-block"]);
        assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()), "{card}");
        let ran = ironwork(&["run", source.path(), "-warnings-block"]);
        assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning, "NO ACCOUNT\n".into()), "{card}");
    }
}

#[test]
fn nocompile_alone_checks_the_program_and_runs_nothing() {
    let source = Source::new("syntax-check", "       CBL NOCOMPILE\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           DISPLAY 'RAN'.\n           GOBACK.\n");
    let checked = ironwork(&["check", source.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(0), String::new()));
    let ran = ironwork(&["run", source.path()]);
    let note = format!("ironwork: {}: NOCOMPILE is a syntax check, with no program to run\n", source.path());
    assert_eq!((ran.status.code(), stderr(&ran), ran.stdout.is_empty()), (Some(241), note, true));
}

fn ending_with(id: &str, last: &str) -> String {
    format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       PROCEDURE DIVISION.\n           DISPLAY 'HELLO'\n{last}")
}

const CICS_NOTE: &str = "informational: IGYPS2091-W not given: the program ends with EXEC CICS RETURN, which the CICS translator turns into a CALL; --cics-return-warning=always gives the warning, =never drops this note";
const NO_END: &str = "warning: no STOP RUN, GOBACK or EXIT PROGRAM in the program: check that it ends";

#[test]
fn cics_return_warning_gives_a_note_once_the_warning_always_or_nothing_never() {
    let cics = Source::new("cics-return", &ending_with("P", "           EXEC CICS RETURN END-EXEC.\n"));
    let lines = |out: &Output| stderr(out).lines().map(|l| l.trim_start_matches(cics.path()).trim_start_matches(": ").to_owned()).collect::<Vec<_>>();
    let default = ironwork(&["check", cics.path()]);
    assert_eq!((default.status.code(), lines(&default)), (Some(0), vec![CICS_NOTE.to_owned()]));
    let once = ironwork(&["check", cics.path(), "--cics-return-warning=once"]);
    assert_eq!((once.status.code(), lines(&once)), (Some(0), vec![CICS_NOTE.to_owned()]));
    let always = ironwork(&["check", cics.path(), "--cics-return-warning=always"]);
    assert_eq!((always.status.code(), lines(&always)), (Some(4), vec![NO_END.to_owned()]));
    let never = ironwork(&["check", cics.path(), "--cics-return-warning=never"]);
    assert_eq!((never.status.code(), lines(&never)), (Some(0), Vec::<String>::new()));
    for bad in ["--cics-return-warning", "--cics-return-warning=sometimes"] {
        let out = ironwork(&["check", cics.path(), bad]);
        assert_eq!(out.status.code(), Some(2), "{bad}");
        assert!(stderr(&out).contains("--cics-return-warning needs =once, =always or =never"), "{bad}");
    }
}

#[test]
fn the_cics_note_is_given_once_for_a_source_of_several_programs() {
    let two = [ending_with("P1", "           EXEC CICS RETURN END-EXEC.\n       END PROGRAM P1.\n"), ending_with("P2", "           EXEC CICS XCTL PROGRAM('P1') END-EXEC.\n       END PROGRAM P2.\n")].concat();
    let source = Source::new("cics-two", &two);
    let out = ironwork(&["check", source.path()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr(&out).matches("IGYPS2091-W not given").count(), 1, "{}", stderr(&out));
}

#[test]
fn a_cics_program_with_goback_gets_nothing_and_a_program_with_no_end_and_no_cics_is_warned_whatever_the_flag() {
    let goback = Source::new("cics-goback", &ending_with("P", "           EXEC CICS RETURN END-EXEC\n           GOBACK.\n"));
    let plain = Source::new("no-end", &ending_with("P", "           MOVE 1 TO RETURN-CODE.\n"));
    for mode in ["--cics-return-warning=once", "--cics-return-warning=always", "--cics-return-warning=never"] {
        let out = ironwork(&["check", goback.path(), mode]);
        assert_eq!((out.status.code(), stderr(&out)), (Some(0), String::new()), "{mode}");
        let out = ironwork(&["check", plain.path(), mode]);
        assert_eq!(out.status.code(), Some(4), "{mode}");
        assert!(stderr(&out).contains(NO_END), "{mode}: {}", stderr(&out));
    }
}

#[test]
fn initial_with_thread_checks_with_return_code_4_and_runs_as_noinitial() {
    let source = Source::new("initial-thread", "       CBL THREAD,INITIAL\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P RECURSIVE.\n       PROCEDURE DIVISION.\n           DISPLAY 'RAN'.\n           GOBACK.\n");
    let warning = format!("{}: warning: INITIAL conflicts with THREAD, which IBM compiles only as NOINITIAL (see C217)\n", source.path());
    let checked = ironwork(&["check", source.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()));
    let ran = ironwork(&["run", source.path()]);
    assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning, "RAN\n".into()));
}

#[test]
fn a_call_of_ceecbldy_under_intdate_lilian_is_a_warning_on_its_line_and_calls_ceedays() {
    let program = |card: &str| {
        format!("{card}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  D.\n           05 D-LEN PIC S9(4) BINARY VALUE 8.\n           05 D-TEXT PIC X(8) VALUE '19950215'.\n       01  PICS.\n           05 P-LEN PIC S9(4) BINARY VALUE 8.\n           05 P-TEXT PIC X(8) VALUE 'YYYYMMDD'.\n       01  L PIC 9(9) BINARY.\n       PROCEDURE DIVISION.\n           CALL 'CEECBLDY' USING D PICS L OMITTED\n           DISPLAY L\n           GOBACK.\n")
    };
    let lilian = Source::new("ceecbldy-lilian", &program("       CBL INTDATE(LILIAN)\n"));
    let warning = format!("{}:14:12: warning: CALL 'CEECBLDY' under INTDATE(LILIAN): CEECBLDY gives an ANSI integer date, which nothing can use under LILIAN, so the CALL is to CEEDAYS\n", lilian.path());
    let checked = ironwork(&["check", lilian.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()));
    let ran = ironwork(&["run", lilian.path()]);
    assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning, "000150604\n".into()));
    let ansi = Source::new("ceecbldy-ansi", &program(""));
    let checked = ironwork(&["check", ansi.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(0), String::new()));
}

#[test]
fn initcheck_warns_at_compile_time_with_return_code_4_and_the_program_runs() {
    let program = |card: &str| {
        format!("{card}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  Y PIC X VALUE '7'.\n       01  Z PIC X.\n       PROCEDURE DIVISION.\n           IF Y > '5'\n             MOVE '2' TO Z\n           END-IF\n           DISPLAY Z\n           GOBACK.\n")
    };
    let strict = Source::new("initcheck-strict", &program("       CBL INITCHECK(STRICT)\n"));
    let warning = format!("{}:12:12: warning: INITCHECK(STRICT): Z may be used uninitialized: a path to this statement does not set it (see C224)\n", strict.path());
    let checked = ironwork(&["check", strict.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()));
    let ran = ironwork(&["run", strict.path()]);
    assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning, "2\n".into()));
    for card in ["       CBL INITCHECK\n", ""] {
        let quiet = Source::new("initcheck-lax", &program(card));
        let checked = ironwork(&["check", quiet.path()]);
        assert_eq!((checked.status.code(), stderr(&checked)), (Some(0), String::new()), "{card}");
    }
}

const VALIDATING: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. V.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  DOC PIC X(8) VALUE '<a>x</a>'.\n       01  OSR PIC X(100).\n       PROCEDURE DIVISION.\n           XML PARSE DOC VALIDATING WITH OSR PROCESSING PROCEDURE P.\n           GOBACK.\n       P.\n           CONTINUE.\n";
const REFUSED: &str = "XML PARSE VALIDATING WITH OSR: the schema is in IBM's Optimized Schema Representation (OSR), which ironwork does not read";

#[test]
fn a_catalogued_message_carries_its_id_in_text_and_json_and_an_uncatalogued_one_none() {
    let refused = Source::new("validating", VALIDATING);
    let path = refused.path();
    let text = ironwork(&["check", path]);
    assert_eq!((text.status.code(), stderr(&text)), (Some(12), format!("{path}:8:26: IWR0001-S {REFUSED}\n")));
    for flags in [&["--diagnostics", "json"][..], &["--diagnostics=json"]] {
        let json = ironwork(&[&["check", path][..], flags].concat());
        let object = format!("{{\"col\":26,\"file\":\"{path}\",\"id\":\"IWR0001\",\"line\":8,\"member\":null,\"message\":\"{REFUSED}\",\"severity\":\"S\"}}\n");
        assert_eq!((json.status.code(), stderr(&json)), (Some(12), object), "{flags:?}");
    }
    let ran = ironwork(&["run", path, "--diagnostics", "json"]);
    assert_eq!(ran.status.code(), Some(241));
    assert!(stderr(&ran).starts_with("{\"col\":26,") && stderr(&ran).ends_with(&not_run(path, 12)), "{}", stderr(&ran));

    let uncatalogued = Source::new("uncatalogued-json", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  X PIC 9(40).\n       PROCEDURE DIVISION.\n           GOBACK.\n");
    let json = ironwork(&["check", uncatalogued.path(), "--diagnostics", "json"]);
    let object = format!("{{\"col\":8,\"file\":\"{}\",\"id\":null,\"line\":5,\"member\":null,\"message\":\"PICTURE 9(40): more than 31 digits\",\"severity\":\"S\"}}\n", uncatalogued.path());
    assert_eq!(stderr(&json), object);
}

#[test]
fn diagnostics_takes_text_or_json_on_the_commands_that_compile() {
    let source = Source::new("diagnostics-flag", VALIDATING);
    assert_eq!(ironwork(&["check", source.path(), "--diagnostics", "text"]).status.code(), Some(12));
    for (args, said) in [
        (&["check", source.path(), "--diagnostics", "xml"][..], "--diagnostics needs text or json"),
        (&["check", source.path(), "--diagnostics"], "--diagnostics needs text or json"),
        (&["assumptions", "--diagnostics", "json"], "--diagnostics is for check, run, cics and compile"),
    ] {
        let o = ironwork(args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(stderr(&o).contains(said), "{args:?}: {}", stderr(&o));
    }
}
