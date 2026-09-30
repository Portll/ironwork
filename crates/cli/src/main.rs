use exec::abend::{AbendCode, Signal};
use std::process::ExitCode;
use std::{env, fs, io};

const USAGE: &str = "ironwork for COBOL
usage:
  ironwork run <program.cbl> [-silent] [-strict-sort-keys] [-warnings-block] [--fastsrt-adv-print=exclude|include]
               [-debug] [-I <dir>]... [-L <dir>]... [--dd NAME=path[:format]]... [--clock <time>]
               [--sql-db URL [--sql-record path] | --sql-replay path [--sql-replay-mode strict|keyed]]
                                                       compile and run; CBL and PROCESS cards set the options
  ironwork check <program.cbl> [-I <dir>]...           compile only
  ironwork cics <program.cbl> [run flags] [--transid T] [--termid T] [--userid U] [--applid A] [--sysid S]
               [--commarea path[:text]] [--commarea-out path[:text]] [--file SPEC]... [--td QUEUE=path]...
               [--screens path | --serve HOST:PORT [--transaction TRAN=PROGRAM]... [--csd path]]
                                                       run as the first program of a CICS task
  ironwork job <job.jcl> --datasets DIR[:text] [--proclib DIR]... [run flags] [-I <dir>]... [-L <dir>]... [--clock <time>] [--sql-replay path]
                                                       run a job's steps in order
  ironwork assumptions [--c-series]                    list the register of assumptions, one per line
  ironwork --version
flags:
  -silent    stop the checked-mode reports: TRUNC(OPT) stores whose result depends on the
             generated code, and SORT statements whose outcome FASTSRT changes
  -strict-sort-keys
             read a SORT or MERGE key as the program would, so a zoned or packed key that is not
             a valid number is a data exception (S0C7); without it, keys compare as DFSORT's ZD
             and PD formats compare them
  --fastsrt-adv-print=exclude|include
             whether FASTSRT gives DFSORT the I/O of a USING or GIVING print file under ADV, whose
             data set's records are a byte longer than its FD's. exclude (the default) leaves it to
             COBOL; include has DFSORT take the records as they stand: it reads the control
             character as a record's first byte and writes none, padding each record with X'00' or
             failing the SORT where DFSORT's rules for record lengths say so
  -warnings-block
             refuse to run a program whose compile gave warnings, as run and cics refuse one whose
             compile gave errors; the return code stays 4. ironwork's own: IBM's FLAG option only
             chooses which messages are listed
  -debug     the Language Environment runtime option DEBUG: a program compiled WITH DEBUGGING
             MODE runs its USE FOR DEBUGGING procedures, which NODEBUG, IBM's default, keeps from
             running. Debugging lines run in such a program either way
  -I <dir>   a copy library for COPY members, searched after the program's own directory
  -L <dir>   a program library: CALL finds a program there by name, after the programs in the
             same source and the program's own directory
  --dd NAME=path[:format]
             the file a DD name stands for, as JCL would give it; DD_NAME in the environment also
             works. Binary files hold z/OS records (fixed, or variable behind 4-byte RDWs); :text
             reads and writes UTF-8 lines through the program's code page. A print file's records
             carry a printer control character, which :text shows as line spacing. An indexed or
             relative file's DD holds its records in key order, as a REPRO unload does. DD SYSIN is
             what ACCEPT reads; without it, ACCEPT reads standard input
  --provenance FILE
             write what the compile read and decided as an in-toto statement with the SLSA
             Provenance v1 predicate: the source and every COPY member by digest, the option cards
             and the options in force, and ironwork's version and digest. Unsigned. run and check
  --evidence DIR
             record the run in a hash-chained journal and ledger in DIR, in cobolwork's evidence
             format: the source and every COPY member by digest, each DD's digest when it is
             opened, closed and at the end, each program CALL loads, and the abend or RETURN-CODE.
             DIR may not be inside the program's directory or a library. run and check only
  --clock YYYY-MM-DDTHH:MM:SS[.hh]
             the time ACCEPT FROM DATE, TIME and FUNCTION CURRENT-DATE report, for a run that must
             repeat; without it they report the system clock in UTC
  --sql-db postgres://user[:password]@host[:port]/database[?option=value&...]
             run EXEC SQL against PostgreSQL. The options are host=/socket/directory,
             sslmode=disable|verify-full and sslrootcert=path.pem; the password may come from
             PGPASSWORD instead. verify-full needs the TLS build (tls/ in the source), which uses it
             by default over TCP. A normal end commits and an abend rolls back
  --sql-record path
             with --sql-db, write each call and its answer to path, for --sql-replay
  --sql-replay path
             answer EXEC SQL from a recording instead of a database. A call the recording does not
             hold next abends SQLR, naming both calls. --sql-db and --sql-replay work with cics,
             and with --serve one database, and one recording, serves every task
  --sql-replay-mode strict|keyed
             strict (the default) answers call n from the recording's call n; keyed answers each
             call from the first unused recorded call with the same statement and inputs
cics flags:
  --transid, --termid, --userid, --applid, --sysid
             who and what started the task, as EIBTRNID, EIBTRMID and ASSIGN report them
  --commarea path[:text]
             the COMMAREA the task starts with, EBCDIC bytes (or UTF-8 text with :text); EIBCALEN is
             its length, and without it EIBCALEN is 0
  --commarea-out path[:text]
             where RETURN's COMMAREA is written
  --file NAME=path,KSDS,key=OFFSET:LENGTH,len=RECLEN[,text|,variable]
  --file NAME=path,RRDS,len=RECLEN[,text|,variable]
             a CICS file: its data set holds the records in key order, as a REPRO unload does, and
             is written back when the task ends
  --td QUEUE=path
             a transient-data queue appended to path as text lines when the task ends
  --screens path
             a 3270 terminal (24x80) played from a script: `type ROW COL text`, `eof ROW COL`,
             `cursor ROW COL`, and an AID key (ENTER, CLEAR, PA1-PA3, PF1-PF24) ending each turn;
             every screen the task sends is printed when it ends. BMS maps are read from the copy
             libraries as NAME.bms
  --serve HOST:PORT
             serve TN3270 on the address, one terminal at a time until interrupted, instead of a
             script. The program runs as --transid's first task with no COMMAREA; RETURN TRANSID
             waits for the operator's next AID key and runs that transaction with the COMMAREA
             RETURN gave. A task that ends without TRANSID, an abend, or a transaction that is not
             defined ends the conversation. Each task is its own unit of work. Not with --screens,
             --commarea or --commarea-out
  --transaction TRAN=PROGRAM
             with --serve, the program a transaction runs: a program of the source, or one found
             through -L. --transid names the given program; each program compiles once
  --csd path
             with --serve, the region's transactions from a CICS system definition, as DFHCSDUP
             reads it: each DEFINE TRANSACTION runs its PROGRAM. --transid and --transaction win
             over it
job flags:
  --datasets DIR[:text]
             the job's data sets: DSN=A.B is the file DIR/A.B and DSN=A.B(M) the file DIR/A.B/M, a
             partitioned data set being a directory of members. They hold z/OS records, or UTF-8
             lines with :text; in-stream data and SYSOUT are always lines. Each EXEC PGM= runs a
             COBOL program found in -L as PGM.cbl or PGM.cob, IEFBR14, IEBGENER without control
             statements, or IDCAMS (DELETE, REPRO, DEFINE CLUSTER, SET, IF and DO, its messages to
             SYSPRINT); DISP creates, keeps and deletes data sets as each step ends, and
             COND and IF/THEN/ELSE choose the steps. A step's DISPLAY output and its SYSOUT DDs go
             to standard output, a line per step to standard error. What the job uses that
             ironwork does not run (PARM, DISP=MOD, generation data groups, SORT, other IDCAMS
             commands and IBM's other programs) is refused before any step runs. Exit status: the highest return
             code, 16 when a step abended or a JCL error ended the job, 2 for a job refused
  --proclib DIR
             a procedure library, searched for cataloged procedures and INCLUDE members after the
             data sets JCLLIB ORDER names: member M is the file DIR/M or DIR/M.jcl. In-stream
             procedures, SET, symbolic parameters and EXEC and DD overrides are expanded
assumptions flags:
  --c-series
             put each entry's number in one C series first, its position in the register, with the
             original id beside it (C36 L1); the stored ids do not change
ddl: ironwork ddl FILE.sql
  write the Db2 for z/OS DDL in FILE as PostgreSQL DDL for the SQL backend's tests: tables, keys
  and indexes, Db2 types mapped as the conversion layer reads them, NOT NULL WITH DEFAULT given its
  Db2 default, and every clause PostgreSQL has no use for (IN, CCSID, BUFFERPOOL, ...) kept as a
  comment
compare flags: ironwork compare --base OLD.cbl --head NEW.cbl [--dd NAME=path]... [--sql-replay file]
  --base, --head
             the two versions of the program; each runs in its own directory on copies of every DD,
             with the same clock (--clock, or 2026-01-01 when none is given), SYSIN and recording
  --expected NAME=path
             compare the head's DD NAME with this file instead of a base run (a translation's check)
  --declare file
             divergences the change means to make, one a line: DD NAME [lines A-B] reason,
             DISPLAY reason, or RETURN-CODE reason
  --statement file
             where the in-toto equivalence statement is written (standard output otherwise)
  exit status 0 equivalent or equivalent as declared, 1 diverged, 3 inconclusive, 2 usage
compile messages go to standard error, errors first, then warnings, then informational messages:
  `path:line:col: message`, `path:line:col: warning: message`, `path:line:col: informational: message`
exit status: for check, and for a run the compile refuses, the compile's return code, the highest
  of its messages' severities as IBM's: 0 none or informational, 4 warnings, 8, 12 or 16 errors; run
  and cics refuse at 8, or at 4 under -warnings-block. Otherwise RETURN-CODE when the run ends
  normally, 16 an abend; 2 usage";

const FLAGS: &[&str] = &["-silent", "-strict-sort-keys", "-warnings-block", "-debug"];
const CICS_OPTIONS: &[&str] = &["--transid", "--termid", "--userid", "--applid", "--sysid", "--commarea", "--commarea-out", "--file", "--td", "--screens", "--serve", "--transaction", "--csd"];

mod compare;
mod ddl;
mod evidence;
mod job;
mod provenance;

fn usage_error(message: &str) -> ExitCode {
    eprintln!("ironwork: {message}\n{USAGE}");
    ExitCode::from(2)
}

fn list_assumptions(c_series: bool) -> ExitCode {
    use numeric::assumptions::{Basis, Oracle};
    for (n, a) in numeric::assumptions::c_series() {
        let basis = match a.basis {
            Basis::Documented => "documented",
            Basis::Recalled => "recalled",
            Basis::Chosen => "chosen",
            Basis::Observed => "observed",
        };
        let oracle = match a.oracle {
            Oracle::Hercules => "hercules",
            Oracle::EnterpriseCobol => "enterprise-cobol",
            Oracle::Db2 => "db2",
        };
        if c_series {
            println!("C{n}\t{}\t{basis}\t{oracle}\t{}", a.id, a.claim);
        } else {
            println!("{}\t{basis}\t{oracle}\t{}", a.id, a.claim);
        }
    }
    ExitCode::SUCCESS
}

/// The interpreter recurses as COBOL PERFORMs and CALLs nest, so it runs on a thread whose stack
/// holds the deepest nesting the run unit allows.
fn main() -> ExitCode {
    std::thread::Builder::new().stack_size(64 << 20).spawn(driver).map_or(ExitCode::from(2), |t| t.join().unwrap_or(ExitCode::from(16)))
}

fn driver() -> ExitCode {
    let mut args = env::args().skip(1);
    let (mut flags, mut rest, mut libraries, mut dds) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut program_dirs, mut clock) = (Vec::new(), exec::unit::Clock::System);
    let mut cics_options: Vec<(String, String)> = Vec::new();
    let (mut replay, mut keyed) = (None, false);
    let (mut sql_db, mut sql_record) = (None, None);
    let mut c_series = false;
    let mut evidence_dir: Option<std::path::PathBuf> = None;
    let mut provenance_file: Option<std::path::PathBuf> = None;
    let (mut compare_base, mut compare_head, mut declare, mut statement) = (None, None, None, None);
    let mut expected: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut datasets: Option<String> = None;
    let mut proclibs: Vec<std::path::PathBuf> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("ironwork for COBOL {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--base" | "--head" | "--declare" | "--statement" => match args.next() {
                Some(v) => {
                    let v = Some(std::path::PathBuf::from(v));
                    match a.as_str() {
                        "--base" => compare_base = v,
                        "--head" => compare_head = v,
                        "--declare" => declare = v,
                        _ => statement = v,
                    }
                }
                None => return usage_error(&format!("{a} needs a path")),
            },
            "--expected" => match args.next().and_then(|v| v.split_once('=').map(|(n, p)| (n.to_string(), std::path::PathBuf::from(p)))) {
                Some(pair) => expected.push(pair),
                None => return usage_error("--expected needs NAME=path"),
            },
            "--provenance" => match args.next() {
                Some(file) => provenance_file = Some(std::path::PathBuf::from(file)),
                None => return usage_error("--provenance needs a file"),
            },
            "--evidence" => match args.next() {
                Some(dir) => evidence_dir = Some(std::path::PathBuf::from(dir)),
                None => return usage_error("--evidence needs a directory"),
            },
            "--datasets" => match args.next() {
                Some(dir) => datasets = Some(dir),
                None => return usage_error("--datasets needs a directory"),
            },
            "--proclib" => match args.next() {
                Some(dir) => proclibs.push(std::path::PathBuf::from(dir)),
                None => return usage_error("--proclib needs a directory"),
            },
            "--dd" => match args.next() {
                Some(spec) => dds.push(spec),
                None => return usage_error("--dd needs NAME=path"),
            },
            "-L" => match args.next() {
                Some(dir) => program_dirs.push(std::path::PathBuf::from(dir)),
                None => return usage_error("-L needs a directory"),
            },
            "--clock" => match args.next().as_deref().map(parse_clock) {
                Some(Some(c)) => clock = c,
                _ => return usage_error("--clock needs YYYY-MM-DDTHH:MM:SS[.hh]"),
            },
            "--sql-replay" => match args.next() {
                Some(file) => replay = Some(file),
                None => return usage_error("--sql-replay needs a recording"),
            },
            "--sql-db" => match args.next() {
                Some(url) => sql_db = Some(url),
                None => return usage_error("--sql-db needs a postgres:// URL"),
            },
            "--sql-record" => match args.next() {
                Some(file) => sql_record = Some(file),
                None => return usage_error("--sql-record needs a path"),
            },
            "--sql-replay-mode" => match args.next().as_deref() {
                Some("strict") => keyed = false,
                Some("keyed") => keyed = true,
                _ => return usage_error("--sql-replay-mode needs strict or keyed"),
            },
            o if CICS_OPTIONS.contains(&o) => match args.next() {
                Some(value) => cics_options.push((a.clone(), value)),
                None => return usage_error(&format!("{o} needs a value")),
            },
            "--c-series" => c_series = true,
            "-I" => match args.next() {
                Some(dir) => libraries.push(std::path::PathBuf::from(dir)),
                None => return usage_error("-I needs a directory"),
            },
            f if f.starts_with("--fastsrt-adv-print") => match f {
                "--fastsrt-adv-print=exclude" | "--fastsrt-adv-print=include" => flags.push(a),
                _ => return usage_error("--fastsrt-adv-print needs =exclude or =include"),
            },
            f if f.starts_with('-') && f.len() > 1 && !FLAGS.contains(&f) => return usage_error(&format!("unknown flag {f}")),
            f if f.starts_with('-') && f.len() > 1 => flags.push(a),
            _ => rest.push(a),
        }
    }
    if rest == ["assumptions"] {
        return list_assumptions(c_series);
    }
    if c_series {
        return usage_error("unknown flag --c-series");
    }
    if let [c, file] = rest.as_slice()
        && c == "ddl"
    {
        return match ddl::convert_file(std::path::Path::new(file)) {
            Ok(sql) => {
                print!("{sql}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("ironwork: ddl {file}: {e}");
                ExitCode::from(2)
            }
        };
    }
    if rest == ["compare"] {
        let Some(head) = compare_head else { return usage_error("compare needs --head") };
        return compare::run(compare::Request { base: compare_base, head, dds, libraries, program_dirs, flags, clock: match clock {
            exec::unit::Clock::System => exec::unit::Clock::Fixed(1_767_225_600, 0),
            fixed => fixed,
        }, replay: replay.map(std::path::PathBuf::from), expected, declare, statement });
    }
    if let [c, file] = rest.as_slice()
        && c == "job"
    {
        let Some(dir) = datasets else { return usage_error("job needs --datasets DIR") };
        if !dds.is_empty() || sql_db.is_some() || evidence_dir.is_some() || provenance_file.is_some() || !cics_options.is_empty() {
            return usage_error("job takes its DDs from the JCL; --dd, --sql-db, --evidence, --provenance and the cics flags are not for job");
        }
        let (dir, text) = match dir.strip_suffix(":text") {
            Some(d) => (d.to_string(), true),
            None => (dir, false),
        };
        return job::run(job::Request { jcl: file.into(), datasets: dir.into(), text, libraries, program_dirs, proclibs, flags, clock, replay: replay.map(std::path::PathBuf::from) });
    }
    if datasets.is_some() || !proclibs.is_empty() {
        return usage_error("--datasets and --proclib are for job");
    }
    if compare_base.is_some() || compare_head.is_some() || !expected.is_empty() || declare.is_some() || statement.is_some() {
        return usage_error("--base, --head, --expected, --declare and --statement are for compare");
    }
    let (command, path) = match rest.as_slice() {
        [c, p] if c == "run" || c == "check" || c == "cics" => (c.as_str(), p.as_str()),
        _ => return usage_error("expected run, check or cics, and one program"),
    };
    let text = match fs::read(path) {
        Ok(bytes) => syntax::copy::decode(&bytes),
        Err(e) => {
            eprintln!("ironwork: {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let own_directory = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    if (evidence_dir.is_some() || provenance_file.is_some()) && command == "cics" {
        return usage_error("--evidence and --provenance are for run and check");
    }
    let reads: Vec<std::path::PathBuf> = std::iter::once(own_directory.clone()).chain(libraries.iter().cloned()).chain(program_dirs.iter().cloned()).collect();
    let mut journal = match &evidence_dir {
        Some(dir) => match evidence::start(dir, &reads, command, path) {
            Ok(j) => Some(j),
            Err(e) => {
                eprintln!("ironwork: --evidence {}: {e}", dir.display());
                return ExitCode::from(2);
            }
        },
        None => None,
    };
    let libraries = syntax::copy::Libraries::new(std::iter::once(own_directory.clone()).chain(libraries).collect()).with_program(std::path::Path::new(path));
    let mut programs = match syntax::parse_all_with(&text, &libraries) {
        Ok(p) => p,
        Err(e) => return evidence::finish(journal, i64::from(report(std::slice::from_ref(&e), path))),
    };
    if let Some(j) = journal.as_mut() {
        evidence::sources(j, &programs[0].sources, path, &reads);
    }
    let first_sources = programs[0].sources.clone();
    let first_cards = programs[0].options.clone();
    let first = programs.remove(0);
    let library = exec::unit::Library {
        programs,
        dirs: std::iter::once(own_directory).chain(program_dirs).collect(),
        copy: libraries,
        flags: flags.clone(),
    };
    let compiled = match exec::compile(first, &flags) {
        Ok(c) => c,
        Err(messages) => return evidence::finish(journal, i64::from(report(&messages, path))),
    };
    let return_code = report(&compiled.diagnostics, path);
    if let Some(file) = &provenance_file {
        let text = provenance::statement(&provenance::Inputs {
            program: path,
            sources: &first_sources,
            cards: &first_cards,
            flags: &flags,
            roots: &reads,
            compiled: &compiled,
            journal_tip: journal.as_ref().map(|j| (j.id.clone(), j.tip().to_string())),
        });
        if let Err(e) = fs::write(file, &text) {
            eprintln!("ironwork: --provenance {}: {e}", file.display());
            return evidence::finish(journal, 2);
        }
        if let Some(j) = journal.as_mut() {
            evidence::output(j, "provenance", text.as_bytes(), file, &reads);
        }
    }
    if command == "check" {
        return evidence::finish(journal, i64::from(return_code));
    }
    let dds = match exec::files::Dds::new(&dds, true) {
        Ok(d) => d,
        Err(e) => return usage_error(&e),
    };
    let mut database = match (replay, sql_db, sql_record) {
        (Some(_), Some(_), _) => return usage_error("--sql-replay and --sql-db are two databases; give one"),
        (_, None, Some(_)) => return usage_error("--sql-record needs --sql-db"),
        (Some(file), None, None) => match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|text| exec::sql::Replay::parse(&text, keyed)) {
            Ok(r) => Some(Box::new(r) as Box<dyn exec::sql::Database>),
            Err(e) => {
                eprintln!("ironwork: --sql-replay {file}: {e}");
                return ExitCode::from(2);
            }
        },
        (None, Some(url), record) => match live_database(&url, record.as_deref()) {
            Ok(db) => Some(db),
            Err(e) => {
                eprintln!("ironwork: {e}");
                return ExitCode::from(2);
            }
        },
        (None, None, None) => None,
    };
    if command == "cics" {
        return run_cics(&compiled, path, library, dds, clock, database, &cics_options);
    }
    let sysin: Box<dyn io::BufRead> = match dds.get("SYSIN") {
        Some(dd) => match fs::File::open(&dd.path) {
            Ok(f) => Box::new(io::BufReader::new(f)),
            Err(e) => {
                eprintln!("ironwork: DD SYSIN {}: {e}", dd.path.display());
                return ExitCode::from(2);
            }
        },
        None => Box::new(io::stdin().lock()),
    };
    let (mut out, mut err) = (io::stdout().lock(), io::stderr());
    let shared = journal.map(|j| std::rc::Rc::new(std::cell::RefCell::new(evidence::Run::new(j, &reads))));
    let observer = shared.clone().map(|run| Box::new(move |event: exec::unit::Event<'_>| run.borrow_mut().observe(event)) as exec::unit::Observer<'_>);
    let ended = compiled.execute_observed(library, dds, Some(sysin), clock, database.as_deref_mut(), &mut out, &mut err, observer);
    let (status, abend) = match &ended {
        Ok((_, return_code)) => (i64::from(*return_code), None),
        Err(exec::Abend { code: AbendCode::Signal(Signal::ClosedOutput), .. }) => (0, None),
        Err(abend) => (16, Some(abend)),
    };
    if let Some(run) = shared {
        let run = std::rc::Rc::try_unwrap(run).map(std::cell::RefCell::into_inner);
        if let Ok(run) = run {
            let file = abend.and_then(|a| compiled.program.sources.get(a.pos.file as usize)).map(String::as_str);
            evidence::finish(Some(run.end(abend.map(|a| (a.code.to_string(), file, i64::from(a.pos.line))))), status);
        }
    }
    match ended {
        Ok((_, return_code)) => ExitCode::from(return_code as u8),
        Err(exec::Abend { code: AbendCode::Signal(Signal::ClosedOutput), .. }) => ExitCode::SUCCESS,
        Err(abend) => report_abend(&compiled, path, &abend),
    }
}

/// A compile's messages as standard error shows them: errors first, then warnings, then
/// informational messages, each in the order the compiler found them.
fn listing(messages: &[syntax::Error], path: &str) -> Vec<String> {
    let mut ordered: Vec<&syntax::Error> = messages.iter().collect();
    ordered.sort_by_key(|m| std::cmp::Reverse(m.severity));
    ordered.into_iter().map(|m| m.place(path)).collect()
}

/// Prints a compile's messages and gives its return code, the highest of theirs.
fn report(messages: &[syntax::Error], path: &str) -> u8 {
    for line in listing(messages, path) {
        eprintln!("{line}");
    }
    syntax::return_code(messages)
}

/// ironwork's own build has no TLS; the build in tls/ compiles this file with `ironwork_tls` set.
#[cfg(not(ironwork_tls))]
fn tls() -> Option<Box<dyn exec::sql::Tls>> {
    None
}

#[cfg(ironwork_tls)]
fn tls() -> Option<Box<dyn exec::sql::Tls>> {
    Some(Box::new(ironwork_tls::Rustls))
}

/// PostgreSQL, or PostgreSQL behind a recorder writing to `record`.
fn live_database(url: &str, record: Option<&str>) -> Result<Box<dyn exec::sql::Database>, String> {
    let postgres = exec::sql::Postgres::connect(url, tls().as_deref()).map_err(|e| format!("--sql-db: {e}"))?;
    let Some(path) = record else { return Ok(Box::new(postgres)) };
    let file = fs::File::create(path).map_err(|e| format!("--sql-record {path}: {e}"))?;
    let source = postgres.source().to_owned();
    let recorder = exec::sql::Recorder::new(Box::new(postgres), Box::new(io::BufWriter::new(file)), &source).map_err(|e| format!("--sql-record {path}: {e}"))?;
    Ok(Box::new(recorder))
}

fn report_abend(compiled: &exec::Compiled, path: &str, abend: &exec::machine::Abend) -> ExitCode {
    let file = compiled.program.sources.get(abend.pos.file as usize).filter(|f| !f.is_empty()).map_or(path, |f| f.as_str());
    eprintln!("{file}:{}: ABEND {}: {}", abend.pos, abend.code, abend.message);
    ExitCode::from(16)
}

/// A task with the identity, files and queues the cics flags give.
fn cics_task(options: &[(String, String)], number: u32) -> Result<exec::cics::Task, String> {
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let mut task = exec::cics::Task {
        transid: get("--transid").unwrap_or_else(|| "TRAN".into()).to_ascii_uppercase(),
        termid: get("--termid").unwrap_or_else(|| "TERM".into()).to_ascii_uppercase(),
        userid: get("--userid").unwrap_or_else(|| "CICSUSER".into()).to_ascii_uppercase(),
        applid: get("--applid").unwrap_or_else(|| "IRONWORK".into()).to_ascii_uppercase(),
        sysid: get("--sysid").unwrap_or_else(|| "IRON".into()).to_ascii_uppercase(),
        number,
        ..Default::default()
    };
    for (name, value) in options {
        match name.as_str() {
            "--file" => {
                let (file, def) = exec::cics::parse_file(value).map_err(|e| format!("--file {e}"))?;
                task.files.insert(file, def);
            }
            "--td" => match value.split_once('=') {
                Some((queue, file)) => {
                    task.td_files.insert(queue.to_ascii_uppercase(), std::path::PathBuf::from(file));
                }
                None => return Err("--td needs QUEUE=path".into()),
            },
            _ => {}
        }
    }
    Ok(task)
}

/// The programs a served terminal's transactions run, each compiled the first time it is needed,
/// and the database every task shares.
struct Transactions {
    library: exec::unit::Library,
    table: std::collections::HashMap<String, String>,
    compiled: std::collections::HashMap<String, std::rc::Rc<exec::Compiled>>,
    database: Option<Box<dyn exec::sql::Database>>,
}

impl Transactions {
    /// The program by name: one of the source's programs, else a member of a -L directory.
    fn program(&mut self, name: &str) -> Result<std::rc::Rc<exec::Compiled>, String> {
        let name = name.to_ascii_uppercase();
        if let Some(c) = self.compiled.get(&name) {
            return Ok(c.clone());
        }
        let found = self.library.programs.iter().position(|p| p.id.eq_ignore_ascii_case(&name));
        let index = match found {
            Some(i) => i,
            None => {
                let names = [name.clone(), name.to_ascii_lowercase()];
                let path = self
                    .library
                    .dirs
                    .iter()
                    .flat_map(|d| names.iter().flat_map(move |n| ["", ".cbl", ".CBL", ".cob", ".COB"].iter().map(move |e| d.join(format!("{n}{e}")))))
                    .find(|p| p.is_file())
                    .ok_or_else(|| format!("program {name} not found"))?;
                let shown = path.display().to_string();
                let text = fs::read(&path).map(|b| syntax::copy::decode(&b)).map_err(|e| format!("{shown}: {e}"))?;
                let parsed = syntax::parse_all_with(&text, &self.library.copy.with_program(&path)).map_err(|e| e.place(&shown))?;
                let at = self.library.programs.len();
                self.library.programs.extend(parsed);
                at
            }
        };
        let compiled = exec::compile(self.library.programs[index].clone(), &self.library.flags)
            .map_err(|errors| format!("{name} does not compile: {}", syntax::most_severe(&errors).map(|e| e.place(&name)).unwrap_or_default()))?;
        let compiled = std::rc::Rc::new(compiled);
        self.compiled.insert(name, compiled.clone());
        Ok(compiled)
    }
}

/// Serves TN3270 on --serve's address, one connection at a time, running pseudo-conversations.
fn serve_cics(
    first: &exec::Compiled,
    mut library: exec::unit::Library,
    dds: exec::files::Dds,
    clock: exec::unit::Clock,
    database: Option<Box<dyn exec::sql::Database>>,
    options: &[(String, String)],
) -> ExitCode {
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let address = get("--serve").unwrap_or_default();
    if let Err(e) = cics_task(options, 1) {
        return usage_error(&e);
    }
    let mut table = std::collections::HashMap::new();
    if let Some(file) = get("--csd") {
        match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|text| syntax::csd::parse(&text).map_err(|e| e.to_string())) {
            Ok(csd) => table.extend(csd.transactions.into_iter().filter_map(|(tran, t)| Some((tran, t.program?)))),
            Err(e) => return usage_error(&format!("--csd {file}: {e}")),
        }
    }
    table.insert(get("--transid").unwrap_or_else(|| "TRAN".into()).to_ascii_uppercase(), first.program.id.to_ascii_uppercase());
    for (_, spec) in options.iter().filter(|(n, _)| n == "--transaction") {
        match spec.split_once('=') {
            Some((tran, program)) if !tran.is_empty() && !program.is_empty() => {
                table.insert(tran.to_ascii_uppercase(), program.to_ascii_uppercase());
            }
            _ => return usage_error("--transaction needs TRAN=PROGRAM"),
        }
    }
    library.programs.insert(0, first.program.clone());
    let page = first.options.code_page();
    let listener = match std::net::TcpListener::bind(&address) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ironwork: --serve {address}: {e}");
            return ExitCode::from(2);
        }
    };
    eprintln!("ironwork: serving TN3270 on {}", listener.local_addr().map_or(address, |a| a.to_string()));
    let mut transactions = Transactions { library, table, compiled: Default::default(), database };
    for connection in listener.incoming() {
        let stream = match connection {
            Ok(s) => s,
            Err(e) => {
                eprintln!("ironwork: accept: {e}");
                continue;
            }
        };
        let peer = stream.peer_addr().map_or_else(|_| "a terminal".to_string(), |a| a.to_string());
        match exec::tn3270::negotiate(stream) {
            Ok(terminal) => {
                eprintln!("ironwork: {peer} connected as {}", terminal.terminal_type);
                converse(&mut transactions, std::rc::Rc::new(std::cell::RefCell::new(terminal)), &dds, clock, options, &|c| page.encode_char(c));
                eprintln!("ironwork: {peer} disconnected");
            }
            Err(e) => eprintln!("ironwork: {peer}: {e}"),
        }
    }
    ExitCode::SUCCESS
}

/// One terminal's session: pseudo-conversations one after another. Each task borrows the terminal
/// through `Shared`, so the loop keeps it between tasks and reads the operator's next input itself.
/// A conversation that ends leaves its last screen, or its error message, on the terminal, and the
/// operator's next key starts the first transaction again, until the terminal disconnects.
fn converse(
    transactions: &mut Transactions,
    terminal: std::rc::Rc<std::cell::RefCell<exec::tn3270::Tn3270>>,
    dds: &exec::files::Dds,
    clock: exec::unit::Clock,
    options: &[(String, String)],
    encode: &dyn Fn(char) -> Option<u8>,
) {
    let show = |text: &str| {
        eprintln!("ironwork: {text}");
        let (_, columns) = exec::cics::Terminal::size(&*terminal.borrow());
        let mut stream = vec![exec::terminal::ERASE_WRITE, exec::terminal::WCC_RESTORE, exec::terminal::SBA];
        stream.extend(exec::terminal::encode_address(0));
        let shown = text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(columns);
        stream.extend(shown.map(|c| encode(c).unwrap_or(0x6F)));
        if let Err(e) = exec::cics::Terminal::send(&mut *terminal.borrow_mut(), &stream) {
            eprintln!("ironwork: {e}");
        }
    };
    loop {
        match conversation(transactions, &terminal, dds, clock, options) {
            Conversation::Ended => {}
            Conversation::Failed(text) => show(&text),
            Conversation::Disconnected => return,
        }
        let received = exec::cics::Terminal::receive(&mut *terminal.borrow_mut());
        match received {
            Ok(Some(_)) => {}
            Ok(None) => return,
            Err(e) => return eprintln!("ironwork: {e}"),
        }
    }
}

enum Conversation {
    Ended,
    Failed(String),
    Disconnected,
}

/// Tasks from the first transaction on, each started by RETURN TRANSID and the operator's next
/// key, until one returns without TRANSID.
fn conversation(
    transactions: &mut Transactions,
    terminal: &std::rc::Rc<std::cell::RefCell<exec::tn3270::Tn3270>>,
    dds: &exec::files::Dds,
    clock: exec::unit::Clock,
    options: &[(String, String)],
) -> Conversation {
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let mut transid = get("--transid").unwrap_or_else(|| "TRAN".into()).to_ascii_uppercase();
    let (mut commarea, mut aid) = (None, None);
    for number in 1.. {
        let Some(program) = transactions.table.get(&transid).cloned() else {
            return Conversation::Failed(format!("TRANSACTION {transid} IS NOT DEFINED"));
        };
        let compiled = match transactions.program(&program) {
            Ok(c) => c,
            Err(e) => return Conversation::Failed(format!("{transid}: {e}")),
        };
        let mut task = match cics_task(options, number) {
            Ok(t) => t,
            Err(e) => return Conversation::Failed(e),
        };
        task.transid = transid.clone();
        task.commarea = commarea.take();
        task.initial_aid = aid;
        task.terminal = Some(Box::new(exec::tn3270::Shared(terminal.clone())));
        eprintln!("ironwork: task {number}: {transid} runs {program}");
        let (mut out, mut err) = (io::stdout().lock(), io::stderr());
        let ran = compiled.execute_cics_with(transactions.library.clone(), dds.clone(), task, clock, transactions.database.as_deref_mut(), &mut out, &mut err);
        drop(out);
        terminal.borrow_mut().discard_pending();
        let task = match ran {
            Ok((_, task)) => task,
            Err(abend) => return Conversation::Failed(format!("{transid}: ABEND {}: {} at {}", abend.code, abend.message, abend.pos)),
        };
        let Some(next) = task.next_transid.as_deref().map(|t| t.trim().to_ascii_uppercase()) else {
            return Conversation::Ended;
        };
        let received = exec::cics::Terminal::receive(&mut *terminal.borrow_mut());
        match received {
            Ok(Some(record)) => {
                aid = record.first().copied();
                terminal.borrow_mut().push_back(record);
            }
            Ok(None) => return Conversation::Disconnected,
            Err(e) => {
                eprintln!("ironwork: {e}");
                return Conversation::Disconnected;
            }
        }
        transid = next;
        commarea = task.returned_commarea;
    }
    Conversation::Ended
}

/// Runs the program as a CICS task built from the cics flags; reports RETURN TRANSID and writes
/// RETURN's COMMAREA where --commarea-out says.
fn run_cics(
    compiled: &exec::Compiled,
    path: &str,
    library: exec::unit::Library,
    dds: exec::files::Dds,
    clock: exec::unit::Clock,
    mut database: Option<Box<dyn exec::sql::Database>>,
    options: &[(String, String)],
) -> ExitCode {
    let page = compiled.options.code_page();
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    if get("--serve").is_some() {
        if get("--screens").is_some() || get("--commarea").is_some() || get("--commarea-out").is_some() {
            return usage_error("--serve cannot be combined with --screens, --commarea or --commarea-out");
        }
        return serve_cics(compiled, library, dds, clock, database, options);
    }
    if get("--transaction").is_some() || get("--csd").is_some() {
        return usage_error("--transaction and --csd need --serve");
    }
    let mut task = match cics_task(options, 1) {
        Ok(t) => t,
        Err(e) => return usage_error(&e),
    };
    let commarea = match get("--commarea") {
        None => None,
        Some(spec) => {
            let (file, text) = spec.strip_suffix(":text").map_or((spec.as_str(), false), |f| (f, true));
            let bytes = match fs::read(file) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("ironwork: --commarea {file}: {e}");
                    return ExitCode::from(2);
                }
            };
            if !text {
                Some(bytes)
            } else {
                match page.encode(String::from_utf8_lossy(&bytes).trim_end_matches(['\n', '\r'])) {
                    Ok(b) => Some(b),
                    Err(e) => return usage_error(&format!("--commarea {file}: {e}")),
                }
            }
        }
    };
    task.commarea = commarea;
    let mut shown = None;
    if let Some(file) = get("--screens") {
        let script = match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| exec::terminal::parse_script(&t)) {
            Ok(s) => s,
            Err(e) => return usage_error(&format!("--screens {file}: {e}")),
        };
        let terminal = exec::terminal::Scripted::new(24, 80, script, page);
        shown = Some(terminal.shown.clone());
        task.terminal = Some(Box::new(terminal));
    }
    let print_screens = || {
        for (n, screen) in shown.iter().flat_map(|s| s.borrow().clone()).enumerate() {
            println!("--- screen {} ---\n{screen}", n + 1);
        }
    };
    let (mut out, mut err) = (io::stdout().lock(), io::stderr());
    let ran = compiled.execute_cics_with(library, dds, task, clock, database.as_deref_mut(), &mut out, &mut err);
    drop(out);
    print_screens();
    match ran {
        Ok((_, task)) => {
            match &task.next_transid {
                Some(t) => eprintln!("ironwork: RETURN TRANSID({t}) with a {}-byte COMMAREA", task.returned_commarea.as_ref().map_or(0, Vec::len)),
                None => eprintln!("ironwork: the task ended"),
            }
            if let (Some(spec), Some(bytes)) = (get("--commarea-out"), &task.returned_commarea) {
                let (file, text) = spec.strip_suffix(":text").map_or((spec.as_str(), false), |f| (f, true));
                let data = if text { format!("{}\n", page.decode(bytes)).into_bytes() } else { bytes.clone() };
                if let Err(e) = fs::write(file, data) {
                    eprintln!("ironwork: --commarea-out {file}: {e}");
                    return ExitCode::from(2);
                }
            }
            ExitCode::SUCCESS
        }
        Err(exec::Abend { code: AbendCode::Signal(Signal::ClosedOutput), .. }) => ExitCode::SUCCESS,
        Err(abend) => report_abend(compiled, path, &abend),
    }
}

/// `YYYY-MM-DDTHH:MM:SS[.hh]` as seconds since the epoch and hundredths, UTC.
fn parse_clock(text: &str) -> Option<exec::unit::Clock> {
    let (date, time) = text.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let (time, hundredths) = match time.split_once('.') {
        Some((t, h)) => (t, h.parse::<u32>().ok()?),
        None => (time, 0),
    };
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (hour, minute, second) = (t.next()??, t.next()??, t.next()??);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 || hundredths > 99 {
        return None;
    }
    let days = exec::calendar::days_from_civil(year, month, day);
    Some(exec::unit::Clock::Fixed(days * exec::calendar::SECONDS_PER_DAY + hour * 3600 + minute * 60 + second, hundredths))
}

#[cfg(test)]
mod tests {
    use super::listing;
    use syntax::{Error, Pos, Severity};

    #[test]
    fn errors_are_listed_first_then_warnings_then_informational_messages() {
        let at = |line: u32, severity: Severity| Error::at(Pos { file: 0, line, col: 8 }, format!("m{line}")).graded(severity);
        let messages = [at(1, Severity::Informational), at(2, Severity::Warning), at(3, Severity::Severe), at(4, Severity::Error), at(5, Severity::Warning), at(6, Severity::Severe)];
        assert_eq!(listing(&messages, "p.cbl"), ["p.cbl:3:8: m3", "p.cbl:6:8: m6", "p.cbl:4:8: m4", "p.cbl:2:8: warning: m2", "p.cbl:5:8: warning: m5", "p.cbl:1:8: informational: m1"]);
        assert_eq!(syntax::return_code(&messages), 12);
    }
}
