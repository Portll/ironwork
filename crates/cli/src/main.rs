use exec::Execute;
use exec::abend::{AbendCode, Signal};
use std::process::ExitCode;
use std::{env, fs, io};

const USAGE: &str = "ironwork for COBOL
usage:
  ironwork run <program.cbl> [-silent] [-strict-sort-keys] [-warnings-block] [--fastsrt-adv-print=exclude|include]
               [-debug] [--cics-return-warning=once|always|never] [-I <dir>]... [-L <dir>]... [--vm]
               [--dd NAME=path[:format][:mod]]... [--clock <time>] [--parm TEXT]
               [--sql-db URL [--sql-record path] | --sql-replay path [--sql-replay-mode strict|keyed]]
                                                       compile and run; CBL and PROCESS cards set the options
  ironwork check <program.cbl> [-warnings-block] [--cics-return-warning=once|always|never] [-I <dir>]...
                                                       compile only
  ironwork cics <program.cbl> [run flags] [--transid T] [--termid T] [--userid U] [--applid A] [--sysid S]
               [--commarea path[:text]] [--commarea-out path[:text]] [--file SPEC]... [--td QUEUE=path]...
               [--screens path | --serve HOST:PORT [--serve-public] [--transaction TRAN=PROGRAM]... [--csd path]]
                                                       run as the first program of a CICS task
  ironwork compile <program.cbl>... [-o <dir>] [--bundle NAME] [--source-prefix DIR] [-silent] [-strict-sort-keys]
               [-warnings-block] [--fastsrt-adv-print=exclude|include] [-debug] [--cics-return-warning=once|always|never]
               [-I <dir>]... [-L <dir>]...                compile and lower each source's programs to a load module
  ironwork dump [--section NAME]... [--strings] [--no-check] <module.iwm>
                                                       print a load module, one fact per line
  ironwork job <job.jcl> --datasets DIR[:text] [--proclib DIR]... [--user ID] [run flags] [-I <dir>]... [-L <dir>]... [--clock <time>] [--sql-replay path]
                                                       run a job's steps in order
  ironwork fuzz [--job] <program.cbl|job.jcl> -o <dir> [--runs N] [--seed N] [--timeout SECONDS] [--root DIR] [--clock <time>]
               [-I <dir>]... [-L <dir>]... [-silent] [-strict-sort-keys] [-debug]
               [--datasets DIR] [--proclib DIR]... [--user ID]
                                                       run a batch program, or with --job a job, on generated input
                                                       and keep each abend
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
             refuse to run a program whose compile gave warnings, as NOCOMPILE(W) would; the return
             code stays 4. A CBL or PROCESS card's COMPILE or NOCOMPILE wins over it
  -debug     the Language Environment runtime option DEBUG: a program compiled WITH DEBUGGING
             MODE runs its USE FOR DEBUGGING procedures, which NODEBUG, IBM's default, keeps from
             running. Debugging lines run in such a program either way
  --cics-return-warning=once|always|never
             what a program with no STOP RUN, GOBACK or EXIT PROGRAM that ends with EXEC CICS
             RETURN or XCTL gets. IBM warns of the missing end (IGYPS2091-W, return code 4):
             always gives that warning; once (the default) gives an informational note in its
             place, once per run, as the CICS translator turns RETURN and XCTL into a CALL;
             never gives nothing. A program with none of these gets the warning whatever the flag
  --vm       run: lower the program and run it on the VM rather than the interpreter. A program
             lowering refuses gets the compile's 12; a run that reaches what the VM does not run
             yet (file I/O, SORT, Report Writer, EXEC CICS and SQL, JSON and XML, LE services,
             INVOKE) stops there with a message naming it and exit status 12. Not with --evidence
  -I <dir>   a copy library for COPY members, searched after the program's own directory
  -L <dir>   a program library: CALL finds a program there by name, after the programs in the
             same source and the program's own directory
  --dd NAME=path[:format][:mod]
             the file a DD name stands for, as JCL would give it; DD_NAME in the environment also
             works. Binary files hold z/OS records (fixed, or variable behind 4-byte RDWs); :text
             reads and writes UTF-8 lines through the program's code page. A print file's records
             carry a printer control character, which :text shows as line spacing. :mod is
             DISP=MOD: OPEN OUTPUT of a sequential file keeps its records and writes after them. An
             indexed or relative file's DD holds its records in key order, as a REPRO unload does.
             DD SYSIN is what ACCEPT reads; without it, ACCEPT reads standard input. DD PRINTER is
             the virtual printer: CALL 'SYSTEM' or 'C$SYSTEM' with an lp or lpr command appends
             the files it names, each a DD, there and returns 0, and runs nothing
  --parm TEXT
             with run, the PARM an EXEC PGM= would give: the program's first USING item addresses
             a halfword length and the arguments before the last slash, as Language Environment
             passes them under CBLOPTS(ON). The run's journal does not record it
  --provenance FILE
             write what the compile read and decided as an in-toto statement with the SLSA
             Provenance v1 predicate: the source and every COPY member by digest, the option cards
             and the options in force, and ironwork's version and digest. Unsigned. run and check
  --evidence DIR
             record the run in a hash-chained journal and ledger in DIR, in cobolwork's evidence
             format: the source and every COPY member by digest, each DD's digest when it is
             opened, closed and at the end, each program CALL loads, and the abend or RETURN-CODE.
             DIR may not be inside the program's directory or a library. run, check, job and cics
             (one task, not --serve); a job's journal holds the JCL, each program's sources, each
             step's DDs and CALLs, the data sets each step left, and a step record with each
             step's outcome
  --coverage FILE
             write which paragraphs the run entered: for each program of the source every
             paragraph with its line and how often control entered it, and for each program CALL
             loaded from a library the paragraphs it reached. run, and job for every step's
             programs
  --trace-marker TEXT
             with --evidence: record each operation an input could steer, and whether TEXT was in
             its operand: a CALL of a variable program name or of an operating-system command
             routine, DISPLAY, and in a CICS task a data item naming a LINK, XCTL or START
             program or transaction, a queue, a RIDFLD or a SYSID, or what WRITEQ TD, WRITE
             OPERATOR or JOURNALNAME, SEND and WEB write. Enter TEXT where the input comes in
             (SYSIN, a DD, the COMMAREA, a replayed row); each operation is recorded once reached
             and once not. run, job and cics
  --trace-statements FILE
             with --evidence: record each start of a statement FILE lists, one FILE:LINE per
             line, a file matched by its name: cobolwork's routes.statements. The first 100
             starts of each are recorded, in the order the run made them, the 100th marked
             capped. run only
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
compile flags:
  -o <dir>   where the modules go, created if missing; the current directory without it. Each
             source's programs make one module, named after the source: PAYROLL.cbl gives
             PAYROLL.iwm. A source that does not compile or lower writes nothing, and a module is
             written whole or not at all. The same source, libraries and options give the same
             bytes; a program that uses FUNCTION WHEN-COMPILED holds the compile time, which is
             SOURCE_DATE_EPOCH's when it is set. -L is taken as run takes it: every CALL is
             resolved by name when it runs, so it does not change the module
  --bundle NAME
             every source's programs in one module, NAME.iwm, with one directory, written only if
             every source compiles and lowers
  --source-prefix DIR
             the directory the debug table puts before each source's file name, which is
             otherwise the file name alone; COPY members are named relative to their library
dump flags:
  --section NAME
             print only the named sections (STRINGS, DIRECTORY, OPTIONS, LAYOUT, LIR, SQL, BMS,
             DEBUG) after the header and the section table
  --strings  print the string table, which the other sections print inline
  --no-check print a section whose checksum differs too; without it the section is not printed
             and the exit status is 1. Exit status 1 for a module the reader refuses, a section
             that does not decode or a checksum that differs, 2 for a file that cannot be read
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
             libraries as NAME.bms. A task that returns TRANSID is followed, on the same screen,
             by that transaction's task, started by the script's next AID key with the COMMAREA
             RETURN gave, until a task ends without TRANSID or the script has no key left
  --serve HOST:PORT
             serve TN3270 on the address, one terminal at a time until interrupted, instead of a
             script. The program runs as --transid's first task with no COMMAREA; RETURN TRANSID
             waits for the operator's next AID key and runs that transaction with the COMMAREA
             RETURN gave. A task that ends without TRANSID, an abend, or a transaction that is not
             defined ends the conversation. Each task is its own unit of work. Not with --screens,
             --commarea or --commarea-out. The server asks for no credentials, so an address that
             is not loopback is refused without --serve-public
  --serve-public
             serve an address other than loopback, to anyone who reaches it
  --transaction TRAN=PROGRAM
             with --serve or --screens, the program a transaction runs: a program of the source,
             or one found through -L. --transid names the given program; each program compiles
             once
  --csd path
             with --serve or --screens, the region's transactions from a CICS system definition,
             as DFHCSDUP reads it: each DEFINE TRANSACTION runs its PROGRAM. --transid and
             --transaction win over it
job flags:
  --datasets DIR[:text]
             the job's data sets: DSN=A.B is the file DIR/A.B and DSN=A.B(M) the file DIR/A.B/M, a
             partitioned data set being a directory of members. They hold z/OS records, or UTF-8
             lines with :text; in-stream data and SYSOUT are always lines. Each EXEC PGM= runs a
             COBOL program found in -L as PGM.cbl or PGM.cob, IEFBR14, IEBGENER without control
             statements, IDCAMS (DELETE, REPRO, DEFINE CLUSTER and GDG, SET, IF and DO, its
             messages to SYSPRINT), or SORT/ICEMAN (SORT, MERGE and COPY with FIELDS in CH, AC, ZD,
             CLO, CSL, CST, PD, BI and FI, SUM FIELDS=NONE, RECORD, INCLUDE and OMIT, INREC and
             OUTREC with BUILD, FIELDS or OVERLAY, OUTFIL with FNAMES, FILES, INCLUDE, OMIT, SAVE
             and the same reformatting; a text data set's lines collate as EBCDIC). A COBOL
             program's first USING item gets the step's PARM as Language Environment passes it,
             a halfword length and the arguments before the last slash. DISP creates, keeps and
             deletes data sets as each step ends, and COND and IF/THEN/ELSE choose the steps. A
             step's DISPLAY output and its SYSOUT DDs go to standard output, a line per step to
             standard error. What the job uses that ironwork does not run (other IDCAMS commands,
             PARM to a utility, DFSORT's IFTHEN and field conversions, and IBM's other programs)
             is refused before any step runs. JOBLIB and STEPLIB are not allocated; a DD that
             names no data set but asks for one (UNIT=, SPACE=) gets a temporary one; data with no
             DD before it is SYSIN. DISP=MOD writes after what a data set or generation holds, and
             creates it, as NEW would, where it is not there. A generation data group's base is the file
             DIR/BASE that DEFINE GDG writes, and generation n the file DIR/BASE.GnnnnV00; (0),
             (-1) and (+1) count from the generations the job began with, DSN=BASE reads them all,
             newest first, and generations past LIMIT roll off, all but the newest under EMPTY.
             DSN=*.stepname.ddname and *.stepname.procstepname.ddname name an earlier DD's data set. Exit status: the highest return
             code, 16 when a step abended or a JCL error ended the job, 2 for a job refused
  --expected DATASETS=DIR
             migration equivalence: the job runs on a copy of --datasets with a fixed clock
             (--clock, or 2026-01-01), and each file in DIR, laid out as --datasets is, is compared
             byte for byte with the data set the job left. --expected STEPS=file adds production's
             step outcomes, one a line (STEP RC=0004, STEP ABEND S0C7), compared with the job's.
             --declare names intended divergences (DATASET DSN [lines A-B] reason, STEP NAME
             reason) and --statement where the in-toto statement goes;
             exit status 0 equivalent, 1 diverged, 3 inconclusive
  --proclib DIR
             a procedure library, searched for cataloged procedures and INCLUDE members after the
             data sets JCLLIB ORDER names: member M is the file DIR/M or DIR/M.jcl. In-stream
             procedures, SET, symbolic parameters and EXEC and DD overrides are expanded
  --user ID
             the user ID that submitted the job: &SYSUID's value when the JOB statement gives no
             USER=
  --step-parm STEP=TEXT
             the PARM step STEP gets in place of its EXEC's, STEP as the job log names it
             (stepname, or stepname.procstepname); the journal records the JCL, not this
  --instream STEP.DD=path
             the lines of path in place of the in-stream data of DD in step STEP
fuzz flags:
  -o <dir>   the fuzz run's directory, which must be new or empty and outside the program's
             directory and its libraries: manifest.json, evidence/ with the journal of a run on each
             kept input, and coverage/ with that run's paragraphs, as cobolwork's abend set reads
             them (docs/evidence.md §5)
  --runs N   how many generated inputs to run, 200 without it
  --seed N   the generator's seed, 1 without it; the same seed gives the same inputs
  --timeout SECONDS
             how long one run may take before it is stopped and counted a timeout, 10 without it
  --root DIR the repository root the manifest names the program from, the current directory without it
  --job      fuzz the job in the JCL file through ironwork job: each data set a COBOL step reads
             before any step creates it is built from that program's file description, each
             in-stream DD a COBOL step reads gets generated lines, and each COBOL step whose program
             takes a PARM gets generated PARMs (--instream and --step-parm). --datasets gives data
             sets every run starts with; a data set the job reads that is neither fed nor given is
             empty. Only an abend placed at a COBOL statement is kept
             The inputs are the sequential, indexed and relative files the program OPENs INPUT
             or I-O on a DD of its own, built field by field from their records' descriptions, an
             indexed file's in key order, a relative file's a record per slot with some empty,
             variable-length records behind RDWs, SYSIN lines where it ACCEPTs from SYSIN, and a
             PARM where its USING item is a halfword length and text.
             Each data set is written inside the fuzz run's directory. An abend the program
             gives on empty input is not kept; any other, by code and place, is kept once with the
             smallest input found that still gives it
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
exit status: for check and compile, and for a run the compile refuses, the compile's return code, the highest
  of its messages' severities as IBM's: 0 none or informational, 4 warnings, 8, 12 or 16 errors; run
  and cics refuse from 12 under IBM's default NOCOMPILE(S), from 4 under -warnings-block, or as a
  card's COMPILE or NOCOMPILE says; compile gives 12 for a program lowering refuses, naming the
  construct and where it is, and 16 for a source it cannot read or a module it cannot write.
  Otherwise RETURN-CODE when the run ends normally, 16 an abend; 2 usage";

const FLAGS: &[&str] = &["-silent", "-strict-sort-keys", "-warnings-block", "-debug"];
const CICS_OPTIONS: &[&str] = &["--transid", "--termid", "--userid", "--applid", "--sysid", "--commarea", "--commarea-out", "--file", "--td", "--screens", "--serve", "--transaction", "--csd"];

mod compare;
mod compile;
mod coverage;
mod ddl;
mod dfsort;
mod dump;
mod evidence;
mod fuzz;
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
    let mut vm = false;
    let mut evidence_dir: Option<std::path::PathBuf> = None;
    let mut trace_marker: Option<String> = None;
    let mut trace_statements: Option<std::path::PathBuf> = None;
    let mut provenance_file: Option<std::path::PathBuf> = None;
    let (mut compare_base, mut compare_head, mut declare, mut statement) = (None, None, None, None);
    let mut expected: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut datasets: Option<String> = None;
    let mut coverage_file: Option<std::path::PathBuf> = None;
    let mut proclibs: Vec<std::path::PathBuf> = Vec::new();
    let mut user: Option<String> = None;
    let (mut out_dir, mut bundle, mut source_prefix): (Option<std::path::PathBuf>, Option<String>, Option<String>) = (None, None, None);
    let mut dump_options = dump::Options { check: true, ..Default::default() };
    let mut clock_text: Option<String> = None;
    let mut fuzz_root: Option<std::path::PathBuf> = None;
    let (mut fuzz_runs, mut fuzz_seed, mut fuzz_timeout): (Option<u32>, Option<u64>, Option<u64>) = (None, None, None);
    let mut parm: Option<String> = None;
    let (mut fuzz_job, mut fuzz_cics) = (false, false);
    let mut step_parms: Vec<(String, String)> = Vec::new();
    let mut instream: Vec<(String, std::path::PathBuf)> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--step-parm" => match args.next().and_then(|v| v.split_once('=').map(|(s, t)| (s.to_ascii_uppercase(), t.to_string()))).filter(|(_, t)| t.chars().count() <= rt::le::parm::PARM_LIMIT) {
                Some(pair) => step_parms.push(pair),
                None => return usage_error(&format!("--step-parm needs STEP=TEXT, the text at most {} characters", rt::le::parm::PARM_LIMIT)),
            },
            "--instream" => match args.next().and_then(|v| v.split_once('=').map(|(k, p)| (k.to_ascii_uppercase(), std::path::PathBuf::from(p)))) {
                Some(pair) => instream.push(pair),
                None => return usage_error("--instream needs STEP.DD=path"),
            },
            "--job" => fuzz_job = true,
            "--cics" => fuzz_cics = true,
            "--parm" => match args.next().filter(|p| p.chars().count() <= rt::le::parm::PARM_LIMIT) {
                Some(p) => parm = Some(p),
                None => return usage_error(&format!("--parm needs the text of a PARM, at most {} characters", rt::le::parm::PARM_LIMIT)),
            },
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
            "--coverage" => match args.next() {
                Some(file) => coverage_file = Some(std::path::PathBuf::from(file)),
                None => return usage_error("--coverage needs a file"),
            },
            "--provenance" => match args.next() {
                Some(file) => provenance_file = Some(std::path::PathBuf::from(file)),
                None => return usage_error("--provenance needs a file"),
            },
            "--evidence" => match args.next() {
                Some(dir) => evidence_dir = Some(std::path::PathBuf::from(dir)),
                None => return usage_error("--evidence needs a directory"),
            },
            "--trace-marker" => match args.next().filter(|m| !m.is_empty()) {
                Some(m) => trace_marker = Some(m),
                None => return usage_error("--trace-marker needs the text entered at the input"),
            },
            "--trace-statements" => match args.next() {
                Some(file) => trace_statements = Some(std::path::PathBuf::from(file)),
                None => return usage_error("--trace-statements needs a file of FILE:LINE statements"),
            },
            "--datasets" => match args.next() {
                Some(dir) => datasets = Some(dir),
                None => return usage_error("--datasets needs a directory"),
            },
            "--proclib" => match args.next() {
                Some(dir) => proclibs.push(std::path::PathBuf::from(dir)),
                None => return usage_error("--proclib needs a directory"),
            },
            "--user" => match args.next().filter(|u| jcl::is_name(u)) {
                Some(id) => user = Some(id),
                None => return usage_error("--user needs a user ID of one to eight upper-case letters, digits, #, $ or @"),
            },
            "--dd" => match args.next() {
                Some(spec) => dds.push(spec),
                None => return usage_error("--dd needs NAME=path"),
            },
            "-L" => match args.next() {
                Some(dir) => program_dirs.push(std::path::PathBuf::from(dir)),
                None => return usage_error("-L needs a directory"),
            },
            "--clock" => match args.next().map(|t| (parse_clock(&t), t)) {
                Some((Some(c), t)) => {
                    clock = c;
                    clock_text = Some(t);
                }
                _ => return usage_error("--clock needs YYYY-MM-DDTHH:MM:SS[.hh]"),
            },
            "--root" => match args.next() {
                Some(dir) => fuzz_root = Some(std::path::PathBuf::from(dir)),
                None => return usage_error("--root needs a directory"),
            },
            "--runs" => match args.next().and_then(|n| n.parse().ok()) {
                Some(n) => fuzz_runs = Some(n),
                None => return usage_error("--runs needs a number"),
            },
            // The manifest records the seed as a JSON integer, so it stays within i64.
            "--seed" => match args.next().and_then(|n| n.parse().ok()).filter(|&n: &u64| i64::try_from(n).is_ok()) {
                Some(n) => fuzz_seed = Some(n),
                None => return usage_error("--seed needs a number from 0 to 9223372036854775807"),
            },
            "--timeout" => match args.next().and_then(|n| n.parse().ok()).filter(|&n: &u64| n > 0) {
                Some(n) => fuzz_timeout = Some(n),
                None => return usage_error("--timeout needs a number of seconds"),
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
            "--serve-public" => cics_options.push((a.clone(), String::new())),
            o if CICS_OPTIONS.contains(&o) => match args.next() {
                Some(value) => cics_options.push((a.clone(), value)),
                None => return usage_error(&format!("{o} needs a value")),
            },
            "-o" => match args.next() {
                Some(dir) => out_dir = Some(std::path::PathBuf::from(dir)),
                None => return usage_error("-o needs a directory"),
            },
            "--bundle" => match args.next() {
                Some(name) => bundle = Some(name),
                None => return usage_error("--bundle needs a module name"),
            },
            "--source-prefix" => match args.next() {
                Some(prefix) => source_prefix = Some(prefix),
                None => return usage_error("--source-prefix needs a directory"),
            },
            "--section" => match args.next().as_deref().map(|n| (n.to_owned(), dump::section_named(n))) {
                Some((_, Some(section))) => dump_options.only.push(section),
                Some((name, None)) => return usage_error(&format!("--section {name}: no such section")),
                None => return usage_error("--section needs a section name"),
            },
            "--strings" => dump_options.strings = true,
            "--no-check" => dump_options.check = false,
            "--c-series" => c_series = true,
            "--vm" => vm = true,
            "-I" => match args.next() {
                Some(dir) => libraries.push(std::path::PathBuf::from(dir)),
                None => return usage_error("-I needs a directory"),
            },
            f if f.starts_with("--fastsrt-adv-print") => match f {
                "--fastsrt-adv-print=exclude" | "--fastsrt-adv-print=include" => flags.push(a),
                _ => return usage_error("--fastsrt-adv-print needs =exclude or =include"),
            },
            f if f.starts_with("--cics-return-warning") => match f {
                "--cics-return-warning=once" | "--cics-return-warning=always" | "--cics-return-warning=never" => flags.push(a),
                _ => return usage_error("--cics-return-warning needs =once, =always or =never"),
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
    let run_flags = !dds.is_empty() || replay.is_some() || keyed || sql_db.is_some() || sql_record.is_some() || evidence_dir.is_some() || trace_marker.is_some()
        || trace_statements.is_some() || provenance_file.is_some() || coverage_file.is_some() || !cics_options.is_empty() || !matches!(clock, exec::unit::Clock::System)
        || compare_base.is_some() || compare_head.is_some() || declare.is_some() || statement.is_some() || !expected.is_empty() || datasets.is_some()
        || !proclibs.is_empty() || user.is_some()
        || vm || parm.is_some();
    let dump_flags = !dump_options.only.is_empty() || dump_options.strings || !dump_options.check;
    let fuzz_flags = fuzz_root.is_some() || fuzz_runs.is_some() || fuzz_seed.is_some() || fuzz_timeout.is_some() || fuzz_job || fuzz_cics;
    if vm && (rest.first().map(String::as_str) != Some("run") || evidence_dir.is_some()) {
        return usage_error("--vm is for run, and not with --evidence");
    }
    if parm.is_some() && rest.first().map(String::as_str) != Some("run") {
        return usage_error("--parm is for run; a job's PARM comes from its EXEC, and fuzz makes its own");
    }
    if (!step_parms.is_empty() || !instream.is_empty()) && rest.first().map(String::as_str) != Some("job") {
        return usage_error("--step-parm and --instream are for job");
    }
    if rest.first().is_some_and(|c| c == "fuzz") {
        let [_, file] = rest.as_slice() else { return usage_error("fuzz needs one program, or one job with --job") };
        let Some(out) = out_dir else { return usage_error("fuzz needs -o DIR") };
        if fuzz_job && fuzz_cics {
            return usage_error("fuzz takes --job or --cics, not both");
        }
        // Fuzz makes every input, DD, journal and coverage report itself; a flag it would not use is
        // refused rather than ignored.
        let made = !dds.is_empty() || replay.is_some() || keyed || sql_db.is_some() || sql_record.is_some() || evidence_dir.is_some() || coverage_file.is_some() || provenance_file.is_some() || trace_marker.is_some() || trace_statements.is_some();
        let elsewhere = compare_base.is_some() || compare_head.is_some() || declare.is_some() || statement.is_some() || !expected.is_empty() || dump_flags || bundle.is_some() || source_prefix.is_some();
        let job_only = !fuzz_job && (!proclibs.is_empty() || user.is_some() || datasets.is_some()) || datasets.as_deref().is_some_and(|d| d.ends_with(":text"));
        let cics_only = !fuzz_cics && !cics_options.is_empty();
        let cics_made = cics_options.iter().any(|(n, _)| matches!(n.as_str(), "--commarea" | "--commarea-out" | "--screens" | "--serve"));
        if made || elsewhere || job_only || cics_only || cics_made {
            let taken = match (fuzz_job, fuzz_cics) {
                (true, _) => ", --datasets (without :text), --proclib, --user",
                (_, true) => ", --transid, --termid, --userid, --applid, --sysid, --transaction, --csd, --file, --td",
                _ => "",
            };
            return usage_error(&format!("fuzz makes its own inputs, DDs, evidence and coverage; it takes -o, --runs, --seed, --timeout, --root, --clock, -I, -L{taken} and the compile flags"));
        }
        let request = fuzz::Request {
            program: file.into(),
            out,
            root: fuzz_root.unwrap_or_else(|| std::path::PathBuf::from(".")),
            runs: fuzz_runs.unwrap_or(200),
            seed: fuzz_seed.unwrap_or(1),
            timeout: std::time::Duration::from_secs(fuzz_timeout.unwrap_or(10)),
            libraries,
            program_dirs,
            flags,
            clock: clock_text.unwrap_or_else(|| "2026-01-01T00:00:00".into()),
        };
        if fuzz_job {
            return fuzz::job::run(fuzz::job::Request { fuzz: request, datasets: datasets.map(std::path::PathBuf::from), proclibs, user });
        }
        if fuzz_cics {
            return usage_error("fuzz --cics is not built yet");
        }
        return fuzz::run(request);
    }
    if fuzz_flags {
        return usage_error("--runs, --seed, --timeout, --root, --job and --cics are for fuzz");
    }
    let compile_flags = out_dir.is_some() || bundle.is_some() || source_prefix.is_some();
    match rest.split_first() {
        Some((c, sources)) if c == "compile" => {
            if sources.is_empty() {
                return usage_error("compile needs a program");
            }
            if run_flags || dump_flags {
                return usage_error("compile takes the compile flags, -I, -L, -o, --bundle and --source-prefix");
            }
            return compile::run(compile::Request {
                sources: sources.iter().map(std::path::PathBuf::from).collect(),
                out: out_dir.unwrap_or_else(|| std::path::PathBuf::from(".")),
                bundle,
                libraries,
                flags,
                source_prefix,
            });
        }
        Some((c, files)) if c == "dump" => {
            let [file] = files else { return usage_error("dump needs one module") };
            if run_flags || compile_flags || !flags.is_empty() || !libraries.is_empty() || !program_dirs.is_empty() {
                return usage_error("dump takes --section, --strings and --no-check");
            }
            return dump::run(dump::Request { file: file.into(), options: dump_options });
        }
        _ if compile_flags => return usage_error("-o, --bundle and --source-prefix are for compile"),
        _ if dump_flags => return usage_error("--section, --strings and --no-check are for dump"),
        _ => {}
    }
    if trace_marker.is_some() && (evidence_dir.is_none() || !matches!(rest.first().map(String::as_str), Some("run" | "job" | "cics"))) {
        return usage_error("--trace-marker goes with --evidence, for run, job and cics");
    }
    if trace_statements.is_some() && (evidence_dir.is_none() || rest.first().map(String::as_str) != Some("run")) {
        return usage_error("--trace-statements goes with --evidence, for run");
    }
    let listed = match trace_statements.as_deref().map(evidence::listed_statements).transpose() {
        Ok(listed) => listed,
        Err(e) => {
            eprintln!("ironwork: --trace-statements {}: {e}", trace_statements.unwrap_or_default().display());
            return ExitCode::from(2);
        }
    };
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
        if !dds.is_empty() || sql_db.is_some() || provenance_file.is_some() || !cics_options.is_empty() || compare_base.is_some() || compare_head.is_some() {
            return usage_error("job takes its DDs from the JCL; --dd, --sql-db, --provenance and the cics flags are not for job");
        }
        let (dir, text) = match dir.strip_suffix(":text") {
            Some(d) => (d.to_string(), true),
            None => (dir, false),
        };
        let (mut expected_dir, mut expected_steps) = (None, None);
        for (name, path) in &expected {
            match name.to_ascii_uppercase().as_str() {
                "DATASETS" => expected_dir = Some(path.clone()),
                "STEPS" => expected_steps = Some(path.clone()),
                _ => return usage_error("job takes --expected DATASETS=DIR and --expected STEPS=file"),
            }
        }
        if expected_steps.is_some() && expected_dir.is_none() {
            return usage_error("--expected STEPS=file goes with --expected DATASETS=DIR");
        }
        let clock = match (clock, &expected_dir) {
            (exec::unit::Clock::System, Some(_)) => exec::unit::Clock::Fixed(1_767_225_600, 0),
            (c, _) => c,
        };
        return job::run(job::Request { jcl: file.into(), datasets: dir.into(), text, libraries, program_dirs, proclibs, user, flags, clock, replay: replay.map(std::path::PathBuf::from), expected: expected_dir, expected_steps, declare, statement, evidence: evidence_dir, trace_marker, parms: step_parms, instream, coverage: coverage_file });
    }
    if datasets.is_some() || !proclibs.is_empty() || user.is_some() {
        return usage_error("--datasets, --proclib and --user are for job");
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
    if coverage_file.is_some() && command != "run" {
        return usage_error("--coverage is for run");
    }
    if command == "cics" && (provenance_file.is_some() || (evidence_dir.is_some() && cics_options.iter().any(|(n, _)| n == "--serve"))) {
        return usage_error("--provenance is for run and check, and --evidence for one task, not --serve");
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
    let outlines: Vec<coverage::Outline> = programs.iter().map(coverage::Outline::of).collect();
    if let Some(j) = journal.as_mut() {
        evidence::sources(j, &programs[0].sources, path, &reads);
    }
    let first_sources = programs[0].sources.clone();
    let first_cards = programs[0].options.clone();
    let first = programs.remove(0);
    // The source's functions and prototypes are compiled with it, as IBM compiles the whole
    // compilation group, so their messages come before the program's.
    let (mut functions_code, mut functions_refused) = (0u8, false);
    for function in programs.iter().filter(|p| p.function.is_some()) {
        let (messages, refused) = exec::compile(function.clone(), &flags).map_or_else(|m| (m, true), |c| (c.diagnostics, false));
        functions_code = functions_code.max(report(&messages, path));
        functions_refused |= refused;
    }
    let library = exec::unit::Library {
        programs,
        dirs: std::iter::once(own_directory).chain(program_dirs).collect(),
        copy: libraries,
        flags: flags.clone(),
        trace_statements: listed.as_ref().map(|l| exec::unit::StatementFilter::Lines(l.iter().map(|&(_, line)| line).collect())),
    };
    let compiled = match exec::compile(first, &flags) {
        Ok(c) => c,
        Err(messages) => {
            let return_code = report(&messages, path).max(functions_code);
            if command != "check" && return_code == 0 {
                eprintln!("ironwork: {path}: NOCOMPILE is a syntax check, with no program to run");
            }
            return evidence::finish(journal, i64::from(return_code));
        }
    };
    let return_code = report(&compiled.diagnostics, path).max(functions_code);
    if functions_refused && command != "check" {
        return evidence::finish(journal, i64::from(return_code));
    }
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
    if compiled.program.function.is_some() {
        eprintln!("ironwork: {path}: FUNCTION-ID {}: the source holds user-defined functions and no program to run", compiled.program.id);
        return evidence::finish(journal, 16);
    }
    let code = match vm.then(|| exec::vm::lowered(&compiled)) {
        None => None,
        Some(Ok(code)) => Some(code),
        Some(Err(e)) => {
            eprintln!("{}", syntax::Error::from(e).place(path));
            return evidence::finish(journal, 12);
        }
    };
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
        let run = journal.map(|j| evidence::Run::new(j, &reads, path, trace_marker.as_deref()));
        return run_cics(&compiled, path, library, dds, clock, database, &cics_options, run);
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
    let shared = journal.map(|j| std::rc::Rc::new(std::cell::RefCell::new(evidence::Run::new(j, &reads, path, trace_marker.as_deref()).with_statements(listed.unwrap_or_default()))));
    let covered = coverage_file.as_ref().map(|_| std::rc::Rc::new(std::cell::RefCell::new(coverage::Coverage::default())));
    let observer = (shared.is_some() || covered.is_some()).then(|| {
        let (run, cov) = (shared.clone(), covered.clone());
        Box::new(move |event: exec::unit::Event<'_>| {
            if let Some(c) = &cov {
                c.borrow_mut().observe(&event);
            }
            if let Some(r) = &run {
                r.borrow_mut().observe(event);
            }
        }) as exec::unit::Observer<'_>
    });
    let ended = match (&code, &parm) {
        (None, Some(p)) => compiled.execute_main(library, dds, Some(sysin), clock, database.as_deref_mut(), &mut out, &mut err, observer, p),
        (None, None) => compiled.execute_observed(library, dds, Some(sysin), clock, database.as_deref_mut(), &mut out, &mut err, observer),
        (Some(code), parm) => match exec::vm::execute(&compiled, code, library, dds, Some(sysin), clock, database.as_deref_mut(), &mut out, &mut err, observer, parm.as_deref(), &mut None) {
            Ok(done) => Ok(done),
            Err(exec::vm::Halt::Abend(abend)) => Err(abend),
            Err(exec::vm::Halt::Unimplemented(what)) => {
                eprintln!("ironwork: {path}: the VM does not run {what} yet; run it without --vm");
                Ok((exec::Ending::EndOfProgram, 12))
            }
        },
    };
    let (status, abend) = match &ended {
        Ok((_, return_code)) => (i64::from(*return_code), None),
        Err(exec::Abend { code: AbendCode::Signal(Signal::ClosedOutput), .. }) => (0, None),
        Err(abend) => (16, Some(abend)),
    };
    if let (Some(file), Some(c)) = (&coverage_file, &covered) {
        let text = format!("{}\n", exec::evidence::canonical(&c.borrow().report(&outlines)));
        if let Err(e) = fs::write(file, text) {
            eprintln!("ironwork: --coverage {}: {e}", file.display());
        }
    }
    if let Some(run) = shared {
        let run = std::rc::Rc::try_unwrap(run).map(std::cell::RefCell::into_inner);
        if let Ok(run) = run {
            let file = abend.and_then(|a| abend_file(&compiled, a));
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

/// The source an abend's position is in: a method's own, else the first program's table.
fn abend_file<'a>(compiled: &'a exec::Compiled, abend: &'a exec::machine::Abend) -> Option<&'a str> {
    abend.file.as_deref().or_else(|| compiled.program.sources.get(abend.pos.file as usize).map(String::as_str))
}

fn report_abend(compiled: &exec::Compiled, path: &str, abend: &exec::machine::Abend) -> ExitCode {
    let file = abend_file(compiled, abend).filter(|f| !f.is_empty()).unwrap_or(path);
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
    let table = match transaction_table(first, options) {
        Ok(t) => t,
        Err(e) => return usage_error(&e),
    };
    // The server asks for no credentials: whoever reaches the port runs the transactions against the
    // files and the database this run was given.
    let public = get("--serve-public").is_some();
    let resolved: Vec<std::net::SocketAddr> = std::net::ToSocketAddrs::to_socket_addrs(address.as_str()).map(Iterator::collect).unwrap_or_default();
    if let Some(open) = resolved.iter().find(|a| !a.ip().is_loopback()).filter(|_| !public) {
        eprintln!("ironwork: --serve {address}: {} is not a loopback address, and the server asks for no credentials; give --serve-public to serve it anyway", open.ip());
        return ExitCode::from(2);
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
    let served = listener.local_addr().map_or(address, |a| a.to_string());
    if public {
        eprintln!("ironwork: serving TN3270 on {served} to anyone who reaches it, without credentials");
    } else {
        eprintln!("ironwork: serving TN3270 on {served}");
    }
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

/// The program each transaction runs: --csd's DEFINE TRANSACTIONs, then --transid for the first
/// program, then each --transaction.
fn transaction_table(first: &exec::Compiled, options: &[(String, String)]) -> Result<std::collections::HashMap<String, String>, String> {
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let mut table = std::collections::HashMap::new();
    if let Some(file) = get("--csd") {
        let csd = fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|text| syntax::csd::parse(&text).map_err(|e| e.to_string())).map_err(|e| format!("--csd {file}: {e}"))?;
        table.extend(csd.transactions.into_iter().filter_map(|(tran, t)| Some((tran, t.program?))));
    }
    table.insert(get("--transid").unwrap_or_else(|| "TRAN".into()).to_ascii_uppercase(), first.program.id.to_ascii_uppercase());
    for (_, spec) in options.iter().filter(|(n, _)| n == "--transaction") {
        match spec.split_once('=') {
            Some((tran, program)) if !tran.is_empty() && !program.is_empty() => {
                table.insert(tran.to_ascii_uppercase(), program.to_ascii_uppercase());
            }
            _ => return Err("--transaction needs TRAN=PROGRAM".into()),
        }
    }
    Ok(table)
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
/// RETURN's COMMAREA where --commarea-out says. With --screens, a task that returns TRANSID is
/// followed by that transaction's task on the same terminal, started by the script's next AID key,
/// until one ends without TRANSID or the script has no key left; one journal records them all.
#[allow(clippy::too_many_arguments)]
fn run_cics(
    compiled: &exec::Compiled,
    path: &str,
    library: exec::unit::Library,
    dds: exec::files::Dds,
    clock: exec::unit::Clock,
    mut database: Option<Box<dyn exec::sql::Database>>,
    options: &[(String, String)],
    evidence: Option<evidence::Run>,
) -> ExitCode {
    let page = compiled.options.code_page();
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    if get("--serve").is_some() {
        if get("--screens").is_some() || get("--commarea").is_some() || get("--commarea-out").is_some() {
            return usage_error("--serve cannot be combined with --screens, --commarea or --commarea-out");
        }
        return serve_cics(compiled, library, dds, clock, database, options);
    }
    if (get("--transaction").is_some() || get("--csd").is_some()) && get("--screens").is_none() {
        return usage_error("--transaction and --csd need --serve or --screens");
    }
    if get("--serve-public").is_some() {
        return usage_error("--serve-public is for --serve");
    }
    let table = match transaction_table(compiled, options) {
        Ok(t) => t,
        Err(e) => return usage_error(&e),
    };
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
    let mut conversation = None;
    if let Some(file) = get("--screens") {
        let script = match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| exec::terminal::parse_script(&t)) {
            Ok(s) => s,
            Err(e) => return usage_error(&format!("--screens {file}: {e}")),
        };
        let terminal = std::rc::Rc::new(std::cell::RefCell::new(exec::terminal::Scripted::new(24, 80, script, page)));
        shown = Some(terminal.borrow().shown.clone());
        task.terminal = Some(Box::new(exec::terminal::SharedScript(terminal.clone())));
        conversation = Some(terminal);
    }
    let print_screens = || {
        for (n, screen) in shown.iter().flat_map(|s| s.borrow().clone()).enumerate() {
            println!("--- screen {} ---\n{screen}", n + 1);
        }
    };
    let mut library = library;
    library.programs.insert(0, compiled.program.clone());
    let mut transactions = Transactions { library, table, compiled: Default::default(), database: database.take() };
    let (mut out, mut err) = (io::stdout().lock(), io::stderr());
    let shared = evidence.map(|run| std::rc::Rc::new(std::cell::RefCell::new(run)));
    let mut current: Option<std::rc::Rc<exec::Compiled>> = None;
    let mut number = 1;
    let ran = loop {
        let program = current.as_deref().unwrap_or(compiled);
        let observer = shared.clone().map(|run| Box::new(move |event: exec::unit::Event<'_>| run.borrow_mut().observe(event)) as exec::unit::Observer<'_>);
        let ran = program.execute_cics_observed(transactions.library.clone(), dds.clone(), task, clock, transactions.database.as_deref_mut(), &mut out, &mut err, observer);
        let (Some(terminal), Ok((_, ended))) = (&conversation, &ran) else { break ran };
        let Some(next) = ended.next_transid.as_deref().map(|t| t.trim().to_ascii_uppercase()) else { break ran };
        let record = match exec::terminal::Terminal::receive(&mut *terminal.borrow_mut()) {
            Ok(Some(r)) => r,
            Ok(None) => break ran,
            Err(e) => {
                eprintln!("ironwork: {e}");
                break ran;
            }
        };
        let Some(name) = transactions.table.get(&next).cloned() else {
            eprintln!("ironwork: TRANSACTION {next} IS NOT DEFINED");
            break ran;
        };
        match transactions.program(&name) {
            Ok(c) => current = Some(c),
            Err(e) => {
                eprintln!("ironwork: {next}: {e}");
                break ran;
            }
        }
        number += 1;
        task = match cics_task(options, number) {
            Ok(t) => t,
            Err(e) => return usage_error(&e),
        };
        eprintln!("ironwork: task {number}: {next} runs {name}");
        task.transid = next;
        task.commarea = ended.returned_commarea.clone();
        task.initial_aid = record.first().copied();
        terminal.borrow_mut().push_back(record);
        task.terminal = Some(Box::new(exec::terminal::SharedScript(terminal.clone())));
    };
    drop(out);
    print_screens();
    let compiled = current.as_deref().unwrap_or(compiled);
    if let Some(run) = shared.and_then(|r| std::rc::Rc::try_unwrap(r).ok()) {
        let abend = ran.as_ref().err().filter(|a| !matches!(a.code, AbendCode::Signal(Signal::ClosedOutput)));
        let file = abend.and_then(|a| abend_file(compiled, a));
        let journal = run.into_inner().end(abend.map(|a| (a.code.to_string(), file, i64::from(a.pos.line))));
        evidence::finish(Some(journal), if abend.is_some() { 16 } else { 0 });
    }
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
