//! `ironwork run --evidence`: the journal records the source, the COPY member, each DD's digest and
//! the CALL and where an abend was, in whichever source, links every record to the one before,
//! reaches the ledger, is refused inside a directory the run reads, verifies under cobolwork, and is
//! the journal the interpreter writes when the program runs on the VM.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-evidence-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::create_dir_all(dir.join("data")).unwrap();
    dir
}

fn write_program(dir: &Path) {
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. EVDEMO.",
        "       ENVIRONMENT DIVISION.",
        "       INPUT-OUTPUT SECTION.",
        "       FILE-CONTROL.",
        "           SELECT IN-FILE ASSIGN TO INFILE.",
        "           SELECT OUT-FILE ASSIGN TO OUTFILE.",
        "       DATA DIVISION.",
        "       FILE SECTION.",
        "       FD IN-FILE.",
        "       COPY INREC.",
        "       FD OUT-FILE.",
        "       01 OUT-REC PIC X(10).",
        "       PROCEDURE DIVISION.",
        "           OPEN INPUT IN-FILE OUTPUT OUT-FILE.",
        "           READ IN-FILE END-READ.",
        "           MOVE IN-REC TO OUT-REC.",
        "           CALL 'HELPER'.",
        "           WRITE OUT-REC.",
        "           CLOSE IN-FILE OUT-FILE.",
        "           GOBACK.",
    ];
    fs::write(dir.join("src/EVDEMO.cbl"), program.join("\n") + "\n").unwrap();
    fs::write(dir.join("src/INREC.cpy"), "       01 IN-REC PIC X(10).\n").unwrap();
    fs::write(dir.join("lib/HELPER.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELPER.\n       PROCEDURE DIVISION.\n           GOBACK.\n").unwrap();
    fs::write(dir.join("data/in.txt"), "HELLOWORLD\n").unwrap();
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    match rest.strip_prefix('"') {
        Some(s) => s.find('"').map(|end| &s[..end]),
        None => Some(&rest[..rest.find([',', '}']).unwrap_or(rest.len())]),
    }
}

#[test]
fn a_run_with_evidence_records_what_it_read_opened_and_loaded() {
    let dir = temp("run");
    write_program(&dir);
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("run")
        .arg(dir.join("src/EVDEMO.cbl"))
        .args(["-L"])
        .arg(dir.join("lib"))
        .arg("--dd")
        .arg(format!("INFILE={}:text", dir.join("data/in.txt").display()))
        .arg("--dd")
        .arg(format!("OUTFILE={}:text", dir.join("data/out.txt").display()))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .status()
        .unwrap();
    assert!(status.success());
    let runs: Vec<_> = fs::read_dir(dir.join("ev/runs")).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(runs.len(), 1);
    let journal = fs::read_to_string(&runs[0]).unwrap();
    let lines: Vec<&str> = journal.lines().collect();
    let kinds: Vec<&str> = lines.iter().map(|l| field(l, "kind").unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"open"));
    assert_eq!(kinds.last(), Some(&"close"));
    assert!(lines.iter().any(|l| field(l, "kind") == Some("input") && field(l, "path") == Some("INREC.cpy")), "the COPY member is an input");
    assert!(lines.iter().any(|l| field(l, "dd") == Some("OUTFILE") && field(l, "event") == Some("end") && field(l, "bytes") == Some("11")), "OUTFILE as the run left it");
    assert!(lines.iter().any(|l| field(l, "kind") == Some("call") && field(l, "program") == Some("HELPER")));
    assert!(!journal.contains("HELLOWORLD"), "no record holds data");
    assert!(!journal.contains(&dir.display().to_string()), "no record holds an absolute path");
    for pair in lines.windows(2) {
        assert_eq!(field(pair[1], "prev"), field(pair[0], "hash"));
    }
    let ledger = fs::read_to_string(dir.join("ev/ledger.jsonl")).unwrap();
    let run = ledger.lines().last().unwrap();
    assert_eq!(field(run, "runTip"), field(lines[lines.len() - 1], "hash"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn check_provenance_names_every_copy_member_and_the_options_in_force() {
    let dir = temp("provenance");
    write_program(&dir);
    let source = fs::read_to_string(dir.join("src/EVDEMO.cbl")).unwrap();
    fs::write(dir.join("src/EVDEMO.cbl"), format!("       CBL TRUNC(BIN)\n{source}")).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("check")
        .arg(dir.join("src/EVDEMO.cbl"))
        .arg("--provenance")
        .arg(dir.join("prov.json"))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .status()
        .unwrap();
    assert!(status.success());
    let text = fs::read_to_string(dir.join("prov.json")).unwrap();
    assert!(text.starts_with("{\"_type\":\"https://in-toto.io/Statement/v1\""));
    assert!(text.contains("\"predicateType\":\"https://slsa.dev/provenance/v1\""));
    assert!(text.contains("\"uri\":\"file:INREC.cpy\""), "the COPY member is a resolved dependency");
    assert!(text.contains("\"optionCards\":[\"TRUNC(BIN)\"]"));
    assert!(text.contains("\"trunc\":\"Bin\""), "the option in force");
    assert!(text.contains("\"name\":\"EVDEMO.cbl\""));
    assert!(!text.contains(&dir.display().to_string()), "no absolute path");
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let journal = fs::read_to_string(run).unwrap();
    assert!(journal.lines().any(|l| field(l, "kind") == Some("output") && field(l, "name") == Some("provenance")), "the journal records the statement by digest");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_evidence_directory_inside_the_program_directory_is_refused() {
    let dir = temp("inside");
    write_program(&dir);
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("check").arg(dir.join("src/EVDEMO.cbl")).arg("--evidence").arg(dir.join("src/ev")).status().unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(!dir.join("src/ev").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_abend_in_a_method_names_the_class_source_on_stderr_and_in_the_journal() {
    let dir = temp("method");
    let lines = |l: &[&str]| l.iter().map(|l| format!("       {l}\n")).collect::<String>();
    let class = lines(&[
        "CBL THREAD,DLL",
        "IDENTIFICATION DIVISION.",
        "CLASS-ID. Divider INHERITS Base.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "REPOSITORY.",
        "    CLASS Base IS \"java.lang.Object\"",
        "    CLASS Divider IS \"Divider\".",
        "IDENTIFICATION DIVISION.",
        "OBJECT.",
        "PROCEDURE DIVISION.",
        "IDENTIFICATION DIVISION.",
        "METHOD-ID. \"divide\".",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  D PIC 9 VALUE 0.",
        "01  Q PIC 9.",
        "PROCEDURE DIVISION.",
        "    DIVIDE 10 BY D GIVING Q.",
        "END METHOD \"divide\".",
        "END OBJECT.",
        "END CLASS Divider.",
    ]);
    let client = lines(&[
        "CBL THREAD,DLL",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. CLIENT RECURSIVE.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "REPOSITORY.",
        "    CLASS Divider IS \"Divider\".",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  T USAGE OBJECT REFERENCE Divider.",
        "PROCEDURE DIVISION.",
        "    INVOKE Divider NEW RETURNING T",
        "    INVOKE T \"divide\"",
        "    GOBACK.",
    ]);
    fs::write(dir.join("lib/Divider.cbl"), class).unwrap();
    fs::write(dir.join("src/CLIENT.cbl"), client).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(dir.join("src/CLIENT.cbl")).arg("-L").arg(dir.join("lib")).arg("--evidence").arg(dir.join("ev")).output().unwrap();
    assert_eq!(out.status.code(), Some(240));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with(&format!("{}:19:", dir.join("lib").join("Divider.cbl").display())) && stderr.contains("ABEND S0CB"), "{stderr}");
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let journal = fs::read_to_string(run).unwrap();
    let abend = journal.lines().find(|l| field(l, "kind") == Some("abend")).unwrap();
    assert_eq!((field(abend, "file"), field(abend, "line")), (Some("Divider.cbl"), Some("19")), "{abend}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_abend_names_the_file_and_line_it_happened_at_in_the_program_or_a_library_program() {
    let dir = temp("abend-file");
    let failing = |id: &str| {
        [
            "       IDENTIFICATION DIVISION.".to_string(),
            format!("       PROGRAM-ID. {id}."),
            "       DATA DIVISION.".into(),
            "       WORKING-STORAGE SECTION.".into(),
            "       01 WS-A PIC X(3) VALUE '***'.".into(),
            "       01 WS-N REDEFINES WS-A PIC 9(3).".into(),
            "       01 WS-T PIC 9(3) VALUE 0.".into(),
            "       PROCEDURE DIVISION.".into(),
            "           ADD WS-N TO WS-T.".into(),
            "           GOBACK.".into(),
        ]
        .join("\n")
            + "\n"
    };
    fs::write(dir.join("src/SELFAB.cbl"), failing("SELFAB")).unwrap();
    fs::write(dir.join("lib/HELPAB.cbl"), failing("HELPAB")).unwrap();
    fs::write(dir.join("src/CALLAB.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CALLAB.\n       PROCEDURE DIVISION.\n           CALL 'HELPAB'.\n           GOBACK.\n").unwrap();
    for (program, file) in [("SELFAB", "SELFAB.cbl"), ("CALLAB", "HELPAB.cbl")] {
        let ev = dir.join(format!("ev-{program}"));
        let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
            .arg("run")
            .arg(dir.join(format!("src/{program}.cbl")))
            .arg("-L")
            .arg(dir.join("lib"))
            .arg("--evidence")
            .arg(&ev)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(240));
        let run = fs::read_dir(ev.join("runs")).unwrap().next().unwrap().unwrap().path();
        let journal = fs::read_to_string(run).unwrap();
        let abend = journal.lines().find(|l| field(l, "kind") == Some("abend")).unwrap();
        assert_eq!((field(abend, "code"), field(abend, "file"), field(abend, "line")), (Some("S0C7"), Some(file), Some("9")), "{program}");
    }
    fs::remove_dir_all(dir).unwrap();
}

fn cobol(lines: &[String]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

/// Program `id` running `statements`, then GOBACK.
fn calls(id: &str, statements: &[&str]) -> Vec<String> {
    let head = ["IDENTIFICATION DIVISION.".to_string(), format!("PROGRAM-ID. {id}."), "PROCEDURE DIVISION.".into()];
    head.into_iter().chain(statements.iter().chain(&["GOBACK."]).map(|s| format!("    {s}"))).collect()
}

/// Program `id`, whose eighth line divides by zero.
fn divides(id: &str) -> Vec<String> {
    let lines = ["IDENTIFICATION DIVISION.", &format!("PROGRAM-ID. {id}."), "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01 D PIC 9 VALUE 0.", "01 Q PIC 9.", "PROCEDURE DIVISION.", "    DIVIDE 10 BY D GIVING Q.", "    GOBACK."];
    lines.iter().map(|l| l.to_string()).collect()
}

fn end(id: &str) -> Vec<String> {
    vec![format!("END PROGRAM {id}.")]
}

#[test]
fn an_abend_in_a_called_program_names_that_programs_source_on_stderr_and_in_the_journal() {
    let dir = temp("called");
    let write = |file: &str, parts: &[Vec<String>]| fs::write(dir.join(file), cobol(&parts.concat())).unwrap();
    write("lib/SUB.cbl", &[divides("SUB")]);
    write("lib/MID.cbl", &[calls("MID", &["CALL 'SUB'."])]);
    write("lib/LIBSUB.cbl", &[calls("LIBSUB", &["CALL 'HELPX'."])]);
    write("lib/PAIRQ.cbl", &[calls("PAIRQ", &[]), end("PAIRQ"), divides("OTHERQ"), end("OTHERQ")]);
    write("src/MAINS.cbl", &[calls("MAINS", &["CALL 'SUB'."])]);
    write("src/MAIN3.cbl", &[calls("MAIN3", &["CALL 'MID'."])]);
    let dynamic = ["IDENTIFICATION DIVISION.", "PROGRAM-ID. MAIND.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01 N PIC X(8) VALUE 'SUB'.", "PROCEDURE DIVISION.", "    CALL N.", "    GOBACK."];
    write("src/MAIND.cbl", &[dynamic.iter().map(|l| l.to_string()).collect()]);
    write("src/MAINC.cbl", &[calls("MAINC", &["CALL 'INNER'."]), divides("INNER"), end("INNER"), end("MAINC")]);
    write("src/MAINX.cbl", &[calls("MAINX", &["CALL 'LIBSUB'."]), end("MAINX"), divides("HELPX"), end("HELPX")]);
    write("src/MAINQ.cbl", &[calls("MAINQ", &["CALL 'PAIRQ'.", "CALL 'OTHERQ'."])]);
    let cases = [
        ("MAINS", "lib/SUB.cbl", 8),
        ("MAIN3", "lib/SUB.cbl", 8),
        ("MAIND", "lib/SUB.cbl", 8),
        ("MAINC", "src/MAINC.cbl", 13),
        ("MAINX", "src/MAINX.cbl", 14),
        ("MAINQ", "lib/PAIRQ.cbl", 13),
    ];
    for (program, file, line) in cases {
        let ev = dir.join(format!("ev-{program}"));
        let run = |extra: &[&std::ffi::OsStr]| Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(dir.join(format!("src/{program}.cbl"))).arg("-L").arg(dir.join("lib")).args(extra).output().unwrap();
        let place = format!("{}:{line}:", dir.join(file.replace('/', std::path::MAIN_SEPARATOR_STR)).display());
        for out in [run(&["--evidence".as_ref(), ev.as_os_str()]), run(&["--vm".as_ref()])] {
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(240), "{program}: {stderr}");
            assert!(stderr.starts_with(&place) && stderr.contains("ABEND S0CB"), "{program}: {stderr}");
        }
        let journal = fs::read_to_string(fs::read_dir(ev.join("runs")).unwrap().next().unwrap().unwrap().path()).unwrap();
        let abend = journal.lines().find(|l| field(l, "kind") == Some("abend")).unwrap();
        let name = file.rsplit('/').next();
        assert_eq!((field(abend, "file"), field(abend, "line")), (name, Some(line.to_string().as_str())), "{program}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_abend_in_a_later_cics_task_names_the_library_source_of_its_program() {
    let dir = temp("cics-task");
    fs::write(dir.join("src/FIRSTP.cbl"), cobol(&calls("FIRSTP", &["EXEC CICS RETURN TRANSID('NEXT') END-EXEC."]))).unwrap();
    fs::write(dir.join("lib/LIBPGM.cbl"), cobol(&divides("LIBPGM"))).unwrap();
    fs::write(dir.join("data/screens"), "ENTER\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("cics")
        .arg(dir.join("src/FIRSTP.cbl"))
        .arg("-L")
        .arg(dir.join("lib"))
        .args(["--transid", "FIRS", "--transaction", "NEXT=LIBPGM", "--screens"])
        .arg(dir.join("data/screens"))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(240), "{stderr}");
    assert!(stderr.lines().any(|l| l.starts_with(&format!("{}:8:", dir.join("lib").join("LIBPGM.cbl").display())) && l.contains("ABEND ASRA")), "{stderr}");
    let journal = fs::read_to_string(fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path()).unwrap();
    let abend = journal.lines().find(|l| field(l, "kind") == Some("abend")).unwrap();
    assert_eq!((field(abend, "file"), field(abend, "line")), (Some("LIBPGM.cbl"), Some("8")), "{abend}");
    fs::remove_dir_all(dir).unwrap();
}

/// A lock as a writer that died holding it a few minutes ago left it.
fn stale_lock(ev: &Path, contents: &str) {
    let lock = fs::File::create(ev.join("ledger.lock")).unwrap();
    std::io::Write::write_all(&mut &lock, contents.as_bytes()).unwrap();
    lock.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(180)).unwrap();
}

/// Run with IRONWORK_COBOLWORK_DIR naming a cobolwork checkout and node on the PATH; CI does.
#[test]
fn cobolworks_verifier_accepts_the_evidence_of_real_runs() {
    let Ok(cobolwork) = std::env::var("IRONWORK_COBOLWORK_DIR") else { return };
    let dir = temp("cobolwork-verify");
    write_program(&dir);
    fs::write(dir.join("src/DIVIDE.cbl"), cobol(&divides("DIVIDE"))).unwrap();
    let dynamic = ["IDENTIFICATION DIVISION.", "PROGRAM-ID. DYNAMIC.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01 N PIC X(8) VALUE 'HELPER'.", "PROCEDURE DIVISION.", "    CALL N.", "    GOBACK."];
    fs::write(dir.join("src/DYNAMIC.cbl"), cobol(&dynamic.map(String::from))).unwrap();
    fs::write(dir.join("data/statements"), "EVDEMO.cbl:17\n").unwrap();
    let ev = dir.join("ev");
    let run = |program: &str, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ironwork"))
            .arg("run")
            .arg(dir.join("src").join(program))
            .arg("-L")
            .arg(dir.join("lib"))
            .arg("--dd")
            .arg(format!("INFILE={}:text", dir.join("data/in.txt").display()))
            .arg("--dd")
            .arg(format!("OUTFILE={}:text", dir.join("data/out.txt").display()))
            .arg("--evidence")
            .arg(&ev)
            .args(extra)
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(run("EVDEMO.cbl", &["--trace-statements", &dir.join("data/statements").display().to_string()]), Some(0));
    stale_lock(&ev, "");
    assert_eq!(run("DIVIDE.cbl", &[]), Some(240));
    let mut gone = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("--version").stdout(std::process::Stdio::null()).spawn().unwrap();
    gone.wait().unwrap();
    let old = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() - 180_000;
    stale_lock(&ev, &format!("{} {old}\n", gone.id()));
    assert_eq!(run("DYNAMIC.cbl", &["--trace-input", "--trace-marker", "HELPER"]), Some(0));
    assert_eq!(run("DIVIDE.cbl", &["--vm", "--trace-input"]), Some(240));
    fs::create_dir_all(dir.join("jcl")).unwrap();
    fs::write(dir.join("jcl/EVJOB.jcl"), "//EVJOB JOB (1),'T',CLASS=A\n//STEP1 EXEC PGM=HELPER\n").unwrap();
    let job = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("job")
        .arg(dir.join("jcl/EVJOB.jcl"))
        .arg("--datasets")
        .arg(format!("{}:text", dir.join("data").display()))
        .arg("-L")
        .arg(dir.join("lib"))
        .arg("--evidence")
        .arg(&ev)
        .output()
        .unwrap();
    assert_eq!(job.status.code(), Some(0), "{}", String::from_utf8_lossy(&job.stderr));
    let check = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("check").arg(dir.join("src/EVDEMO.cbl")).arg("--provenance").arg(dir.join("prov.json")).arg("--evidence").arg(&ev).status().unwrap();
    assert!(check.success());

    let lines: Vec<String> = fs::read_dir(ev.join("runs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .chain([ev.join("ledger.jsonl")])
        .flat_map(|p| fs::read_to_string(p).unwrap().lines().map(String::from).collect::<Vec<_>>())
        .collect();
    let mut kinds: Vec<&str> = lines.iter().map(|l| field(l, "kind").unwrap()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds, ["abend", "call", "close", "dd", "genesis", "input", "lock-broken", "open", "output", "run", "sink", "statement", "step"]);
    assert!(lines.iter().any(|l| field(l, "kind") == Some("sink") && field(l, "marker") == Some("HELPER") && field(l, "reached").is_some()), "a sink record with the marker");

    let out = Command::new("node")
        .arg(Path::new(&cobolwork).join("bin/cobolwork.mjs"))
        .args(["evidence", "verify", "--evidence"])
        .arg(&ev)
        .current_dir(&dir)
        .output()
        .expect("node, to run cobolwork's verifier");
    let shown = String::from_utf8_lossy(&out.stdout);
    // 3 is cobolwork's undetermined: every chain verifies, and no witness was named to seal them.
    assert_eq!(out.status.code(), Some(3), "{shown}{}", String::from_utf8_lossy(&out.stderr));
    for verdict in [r#""verified": true"#, r#""broken": []"#, r#""unrecorded": []"#, r#""open": []"#] {
        assert!(shown.contains(verdict), "{verdict} in {shown}");
    }
    fs::remove_dir_all(dir).unwrap();
}

fn source(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

fn put(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A journal's records without what each run's journal has of its own: when each record was
/// written, the chain and hashes that link them, and how long the run took; and with `--vm` left
/// out of `argv`.
fn records(ev: &Path) -> Vec<String> {
    let runs: Vec<PathBuf> = fs::read_dir(ev.join("runs")).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let own = |line: &str| {
        let mut kept = line.replace("\"--vm\",", "");
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

/// Runs `args` from `dir` with `--evidence` and `--coverage` on the interpreter, then on the VM,
/// `{tag}` in an argument naming each run's own files. The two agree on the exit status, standard
/// output and error, the coverage report, each file `written` names and the journal, but for
/// `--vm` in its `argv`; the interpreter's journal records.
fn on_both_executors(dir: &Path, args: &[&str], written: &[&str]) -> Vec<String> {
    let run = |tag: &str| {
        let _ = fs::remove_dir_all(dir.join(format!("ev-{tag}")));
        let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
        command.current_dir(dir).args(args.iter().map(|a| a.replace("{tag}", tag)));
        command.args(["--evidence", &format!("ev-{tag}"), "--coverage", &format!("coverage-{tag}.json")]);
        if tag == "vm" {
            command.arg("--vm");
        }
        command.output().unwrap()
    };
    let (walker, vm) = (run("walker"), run("vm"));
    let shown = |o: &std::process::Output| (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned());
    assert_eq!(shown(&vm), shown(&walker), "{args:?}");
    let read = |file: String| fs::read_to_string(dir.join(&file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    assert_eq!(read("coverage-vm.json".into()), read("coverage-walker.json".into()), "{args:?}");
    for file in written {
        assert_eq!(read(file.replace("{tag}", "vm")), read(file.replace("{tag}", "walker")), "{file}");
    }
    let journal = records(&dir.join("ev-walker"));
    assert_eq!(records(&dir.join("ev-vm")), journal, "{args:?}");
    let argv = fs::read_to_string(fs::read_dir(dir.join("ev-vm/runs")).unwrap().next().unwrap().unwrap().path()).unwrap();
    assert!(argv.lines().next().is_some_and(|open| open.contains("\"--vm\",")), "the VM's argv names --vm");
    journal
}

/// Asserts that each of `kinds` is part of some record of `journal`, so the comparison covers it.
fn holds(journal: &[String], kinds: &[&str]) {
    for kind in kinds {
        assert!(journal.iter().any(|r| r.contains(kind)), "{kind}\n{journal:#?}");
    }
}

#[test]
fn a_batch_run_on_the_vm_writes_the_journal_and_coverage_the_interpreter_writes() {
    let dir = temp("vm-batch");
    put(
        &dir,
        "src/VMEVD.cbl",
        &source(&[
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
            "IDENTIFICATION DIVISION.",
            "FUNCTION-ID. TRIPLE AS 'trp' IS PROTOTYPE.",
            "DATA DIVISION.",
            "LINKAGE SECTION.",
            "01  N PIC 9(3).",
            "01  R PIC 9(4).",
            "PROCEDURE DIVISION USING N RETURNING R.",
            "END FUNCTION TRIPLE.",
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. VMEVD.",
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
            "END PROGRAM VMEVD.",
        ]),
    );
    put(&dir, "copy/sys/INREC.cpy", &source(&["01 IN-REC PIC X(10)."]));
    put(&dir, "copy/BADNUM.cpy", &source(&["01 WS-A PIC X(3) VALUE '***'.", "01 WS-N REDEFINES WS-A PIC 9(3).", "01 WS-T PIC 9(3) VALUE 0."]));
    put(&dir, "copy/ADDBAD.cpy", &source(&["    ADD WS-N TO WS-T."]));
    let helper = ["IDENTIFICATION DIVISION.", "PROGRAM-ID. HELPER.", "PROCEDURE DIVISION.", "    DISPLAY 'HELPER'", "    CALL 'HELPIN'", "    GOBACK.", "IDENTIFICATION DIVISION.", "PROGRAM-ID. HELPIN.", "PROCEDURE DIVISION.", "    DISPLAY 'HELPIN'", "    GOBACK.", "END PROGRAM HELPIN.", "END PROGRAM HELPER."];
    put(&dir, "lib/HELPER.cbl", &source(&helper));
    put(
        &dir,
        "lib/TRP.cbl",
        &source(&["IDENTIFICATION DIVISION.", "FUNCTION-ID. TRIPLE AS 'trp'.", "DATA DIVISION.", "LINKAGE SECTION.", "01  N PIC 9(3).", "01  R PIC 9(4).", "PROCEDURE DIVISION USING N RETURNING R.", "    COMPUTE R = N * 3", "    GOBACK.", "END FUNCTION TRIPLE."]),
    );
    // The VM's CALL takes HELPER from its load module and TRIPLE from source; the interpreter takes both from source.
    let compiled = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(&dir).args(["compile", "lib/HELPER.cbl", "-o", "lib"]).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    put(&dir, "data/in.txt", "HELLOWORLD\n");
    put(&dir, "statements", &(1..=61).map(|n| format!("VMEVD.cbl:{n}\n")).chain(["ADDBAD.cpy:1\n".into(), "HELPER.cbl:4\n".into(), "HELPER.cbl:10\n".into(), "TRP.cbl:8\n".into()]).collect::<String>());
    let args = [
        "run", "src/VMEVD.cbl", "-I", "copy", "-I", "copy/sys", "-L", "lib", "--dd", "INFILE=data/in.txt:text", "--dd", "OUTFILE=data/out-{tag}.txt:text", "--trace-statements", "statements", "--trace-marker", "HELLO", "--trace-input",
    ];
    let journal = on_both_executors(&dir, &args, &["data/out-{tag}.txt"]);
    holds(
        &journal,
        &[
            "\"kind\":\"input\",\"path\":\"INREC.cpy\",\"root\":2",
            "\"from\":\"HELPER.cbl\",\"kind\":\"call\",\"program\":\"HELPER\"",
            "\"from\":\"TRP.cbl\",\"kind\":\"call\",\"program\":\"TRP\"",
            "\"dd\":\"OUTFILE\",\"event\":\"end\"",
            "\"file\":\"HELPER.cbl\",\"kind\":\"statement\",\"line\":10",
            "\"file\":\"ADDBAD.cpy\",\"kind\":\"statement\",\"line\":1",
            "\"file\":\"VMEVD.cbl\",\"input\":true,\"kind\":\"sink\",\"line\":42,\"marker\":\"HELLO\",\"reached\":true",
            "\"code\":\"S0C7\",\"file\":\"ADDBAD.cpy\",\"kind\":\"abend\",\"line\":1",
            "\"exit\":240",
        ],
    );

    put(
        &dir,
        "src/VMTAINT.cbl",
        &source(&[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. VMTAINT.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01 WS-IN PIC X(8).",
            "01 WS-NUM PIC 9(4) VALUE 0.",
            "01 WS-OUT PIC 9(5).",
            "01 WS-PGM PIC X(8).",
            "01 I PIC 9(3) VALUE 0.",
            "PROCEDURE DIVISION.",
            "MAIN-PARA.",
            "    ACCEPT WS-IN",
            "    ACCEPT WS-NUM",
            "    PERFORM VARYING I FROM 1 BY 1 UNTIL I > 3",
            "        DISPLAY 'LOOP ' I",
            "    END-PERFORM",
            "    COMPUTE WS-OUT = WS-NUM * 3",
            "    DISPLAY WS-OUT",
            "    MOVE WS-IN TO WS-PGM",
            "    CALL WS-PGM ON EXCEPTION DISPLAY 'NO ' WS-PGM END-CALL",
            "    MOVE 'SAFE' TO WS-IN",
            "    DISPLAY WS-IN",
            "    MOVE 4 TO RETURN-CODE",
            "    GOBACK.",
        ]),
    );
    put(&dir, "data/sysin.txt", "HELLO\n0042\n");
    put(&dir, "taint-statements", "VMTAINT.cbl:15\nVMTAINT.cbl:17\nVMTAINT.cbl:20\n");
    let args = ["run", "src/VMTAINT.cbl", "--dd", "SYSIN=data/sysin.txt:text", "--trace-statements", "taint-statements", "--trace-marker", "HELLO", "--trace-input"];
    let journal = on_both_executors(&dir, &args, &[]);
    holds(
        &journal,
        &[
            "\"file\":\"VMTAINT.cbl\",\"input\":true,\"kind\":\"sink\",\"line\":18,\"marker\":\"HELLO\",\"reached\":false",
            "\"sink\":\"dynamic-program-load\"",
            "\"file\":\"VMTAINT.cbl\",\"input\":true,\"kind\":\"sink\",\"line\":20,\"marker\":\"HELLO\",\"reached\":true",
            "\"file\":\"VMTAINT.cbl\",\"input\":false,\"kind\":\"sink\",\"line\":22",
            "\"file\":\"VMTAINT.cbl\",\"kind\":\"statement\",\"line\":15",
            "\"exit\":4",
        ],
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_cics_conversation_on_the_vm_writes_the_journal_and_coverage_the_interpreter_writes() {
    let dir = temp("vm-cics");
    put(
        &dir,
        "src/FIRSTP.cbl",
        &source(&[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. FIRSTP.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  WS-AREA PIC X(10) VALUE 'FROMFIRST '.",
            "01  WS-KEY PIC X(5) VALUE '00002'.",
            "01  WS-REC PIC X(15).",
            "LINKAGE SECTION.",
            "01  DFHCOMMAREA PIC X(10).",
            "PROCEDURE DIVISION.",
            "MAIN-PARA.",
            "    IF EIBCALEN > 0",
            "        DISPLAY 'GOT ' DFHCOMMAREA",
            "    END-IF",
            "    EXEC CICS READ FILE('CUSTF') INTO(WS-REC)",
            "        RIDFLD(WS-KEY) END-EXEC",
            "    DISPLAY 'READ ' WS-REC",
            "    EXEC CICS WRITEQ TD QUEUE('LOGQ') FROM(DFHCOMMAREA)",
            "        LENGTH(10) END-EXEC",
            "    EXEC CICS LINK PROGRAM('HELPER') END-EXEC",
            "    PERFORM SHOW",
            "    EXEC CICS RETURN TRANSID('NEXT') COMMAREA(WS-AREA)",
            "        END-EXEC.",
            "SHOW.",
            "    DISPLAY 'FIRST DONE'.",
        ]),
    );
    put(&dir, "lib/HELPER.cbl", &source(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. HELPER.", "PROCEDURE DIVISION.", "H1.", "    DISPLAY 'HELPER'", "    EXEC CICS RETURN END-EXEC."]));
    put(
        &dir,
        "lib/LIBPGM.cbl",
        &source(&[
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
        ]),
    );
    for tag in ["walker", "vm"] {
        put(&dir, &format!("data/custf-{tag}.txt"), "00001ALICE     \n00002BOB       \n");
    }
    put(&dir, "data/comm", "HELLO     \n");
    put(&dir, "data/screens", "ENTER\n");
    put(&dir, "statements", "FIRSTP.cbl:17\nFIRSTP.cbl:25\nHELPER.cbl:5\nLIBPGM.cbl:12\n");
    let args = [
        "cics", "src/FIRSTP.cbl", "-L", "lib", "--transid", "FIRS", "--termid", "T001", "--file", "CUSTF=data/custf-{tag}.txt,KSDS,key=0:5,len=15,text", "--td", "LOGQ=data/td-{tag}.txt", "--commarea", "data/comm:text",
        "--commarea-out", "data/comm-{tag}:text", "--screens", "data/screens", "--transaction", "NEXT=LIBPGM", "--trace-input", "--trace-marker", "HELLO", "--trace-statements", "statements",
    ];
    let journal = on_both_executors(&dir, &args, &["data/td-{tag}.txt"]);
    holds(
        &journal,
        &[
            "\"from\":\"HELPER.cbl\",\"kind\":\"call\",\"program\":\"HELPER\"",
            "\"file\":\"FIRSTP.cbl\",\"input\":true,\"kind\":\"sink\",\"line\":18,\"marker\":\"HELLO\",\"reached\":true",
            "\"file\":\"LIBPGM.cbl\",\"kind\":\"statement\",\"line\":12",
            "\"code\":\"ASRA\",\"file\":\"LIBPGM.cbl\",\"kind\":\"abend\",\"line\":12",
        ],
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_run_the_vm_stops_or_does_not_generate_closes_its_journal_with_that_exit() {
    let dir = temp("vm-stops");
    put(&dir, "src/UUID.cbl", &source(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. UUID.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01 U PIC X(36).", "PROCEDURE DIVISION.", "    MOVE FUNCTION UUID4 TO U.", "    GOBACK."]));
    put(
        &dir,
        "src/MIXED.cbl",
        &format!(
            "       CBL NUMCHECK\n{}",
            source(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. MIXED.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01 N PIC 9(3) VALUE 5.", "PROCEDURE DIVISION.", "    IF N = ALL ZERO DISPLAY 'Z' END-IF.", "    STOP RUN."])
        ),
    );
    for (command, program, status) in [("run", "src/UUID.cbl", 243), ("cics", "src/UUID.cbl", 243), ("run", "src/MIXED.cbl", 242), ("cics", "src/MIXED.cbl", 242)] {
        let ev = format!("ev-{command}-{status}");
        let out = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(&dir).args([command, program, "--vm", "--evidence", &ev, "--trace-input"]).output().unwrap();
        assert_eq!(out.status.code(), Some(status), "{command} {program}: {}", String::from_utf8_lossy(&out.stderr));
        let journal = fs::read_to_string(fs::read_dir(dir.join(&ev).join("runs")).unwrap().next().unwrap().unwrap().path()).unwrap();
        let close = journal.lines().last().unwrap();
        assert_eq!((field(close, "kind"), field(close, "exit")), (Some("close"), Some(status.to_string().as_str())), "{command} {program}");
        assert!(!journal.lines().any(|l| field(l, "kind") == Some("abend")), "{command} {program}");
        let ledger = fs::read_to_string(dir.join(&ev).join("ledger.jsonl")).unwrap();
        assert_eq!(field(ledger.lines().last().unwrap(), "runTip"), field(close, "hash"), "{command} {program}");
    }
    fs::remove_dir_all(dir).unwrap();
}
