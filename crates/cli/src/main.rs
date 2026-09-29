use std::process::ExitCode;
use std::{env, fs, io};

const USAGE: &str = "ironwork for COBOL
usage:
  ironwork run <program.cbl> [-silent] [-I <dir>]... [-L <dir>]... [--dd NAME=path[:format]]... [--clock <time>]
                                                       compile and run; CBL and PROCESS cards set the options
  ironwork check <program.cbl> [-I <dir>]...           compile only
  ironwork cics <program.cbl> [run flags] [--transid T] [--termid T] [--userid U] [--applid A] [--sysid S]
               [--commarea path[:text]] [--commarea-out path[:text]] [--file SPEC]... [--td QUEUE=path]...
                                                       run as the first program of a CICS task
  ironwork --version
flags:
  -silent    stop reporting TRUNC(OPT) stores whose result depends on the generated code
  -I <dir>   a copy library for COPY members, searched after the program's own directory
  -L <dir>   a program library: CALL finds a program there by name, after the programs in the
             same source and the program's own directory
  --dd NAME=path[:format]
             the file a DD name stands for, as JCL would give it; DD_NAME in the environment also
             works. Binary files hold z/OS records (fixed, or variable behind 4-byte RDWs); :text
             reads and writes UTF-8 lines through the program's code page. An indexed or relative
             file's DD holds its records in key order, as a REPRO unload does. DD SYSIN is what
             ACCEPT reads; without it, ACCEPT reads standard input
  --clock YYYY-MM-DDTHH:MM:SS[.hh]
             the time ACCEPT FROM DATE, TIME and FUNCTION CURRENT-DATE report, for a run that must
             repeat; without it they report the system clock in UTC
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
exit status: RETURN-CODE when the run ends normally; 12 compile errors, 16 an abend, 2 usage";

const FLAGS: &[&str] = &["-silent"];
const CICS_OPTIONS: &[&str] = &["--transid", "--termid", "--userid", "--applid", "--sysid", "--commarea", "--commarea-out", "--file", "--td"];

fn usage_error(message: &str) -> ExitCode {
    eprintln!("ironwork: {message}\n{USAGE}");
    ExitCode::from(2)
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
            o if CICS_OPTIONS.contains(&o) => match args.next() {
                Some(value) => cics_options.push((a.clone(), value)),
                None => return usage_error(&format!("{o} needs a value")),
            },
            "-I" => match args.next() {
                Some(dir) => libraries.push(std::path::PathBuf::from(dir)),
                None => return usage_error("-I needs a directory"),
            },
            f if f.starts_with('-') && f.len() > 1 && !FLAGS.contains(&f) => return usage_error(&format!("unknown flag {f}")),
            f if f.starts_with('-') && f.len() > 1 => flags.push(a),
            _ => rest.push(a),
        }
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
    let libraries = syntax::copy::Libraries::new(std::iter::once(own_directory.clone()).chain(libraries).collect());
    let mut programs = match syntax::parse_all_with(&text, &libraries) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e.place(path));
            return ExitCode::from(12);
        }
    };
    let first = programs.remove(0);
    let library = exec::unit::Library {
        programs,
        dirs: std::iter::once(own_directory).chain(program_dirs).collect(),
        copy: libraries,
        flags: flags.clone(),
    };
    let compiled = match exec::compile(first, &flags) {
        Ok(c) => c,
        Err(errors) => {
            for e in errors {
                eprintln!("{}", e.place(path));
            }
            return ExitCode::from(12);
        }
    };
    if command == "check" {
        return ExitCode::SUCCESS;
    }
    let dds = match exec::files::Dds::new(&dds, true) {
        Ok(d) => d,
        Err(e) => return usage_error(&e),
    };
    if command == "cics" {
        return run_cics(&compiled, path, library, dds, clock, &cics_options);
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
    match compiled.execute(library, dds, Some(sysin), clock, &mut out, &mut err) {
        Ok((_, return_code)) => ExitCode::from(return_code as u8),
        Err(abend) if abend.code == exec::machine::CLOSED_OUTPUT => ExitCode::SUCCESS,
        Err(abend) => report_abend(&compiled, path, &abend),
    }
}

fn report_abend(compiled: &exec::Compiled, path: &str, abend: &exec::machine::Abend) -> ExitCode {
    let file = compiled.program.sources.get(abend.pos.file as usize).filter(|f| !f.is_empty()).map_or(path, |f| f.as_str());
    eprintln!("{file}:{}: ABEND {}: {}", abend.pos, abend.code, abend.message);
    ExitCode::from(16)
}

/// Runs the program as a CICS task built from the cics flags; reports RETURN TRANSID and writes
/// RETURN's COMMAREA where --commarea-out says.
fn run_cics(compiled: &exec::Compiled, path: &str, library: exec::unit::Library, dds: exec::files::Dds, clock: exec::unit::Clock, options: &[(String, String)]) -> ExitCode {
    let page = compiled.options.code_page();
    let get = |name: &str| options.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let mut task = exec::cics::Task {
        transid: get("--transid").unwrap_or_else(|| "TRAN".into()).to_ascii_uppercase(),
        termid: get("--termid").unwrap_or_else(|| "TERM".into()).to_ascii_uppercase(),
        userid: get("--userid").unwrap_or_else(|| "CICSUSER".into()).to_ascii_uppercase(),
        applid: get("--applid").unwrap_or_else(|| "IRONWORK".into()).to_ascii_uppercase(),
        sysid: get("--sysid").unwrap_or_else(|| "IRON".into()).to_ascii_uppercase(),
        number: 1,
        ..Default::default()
    };
    for (name, value) in options {
        match name.as_str() {
            "--file" => match exec::cics::parse_file(value) {
                Ok((file, def)) => {
                    task.files.insert(file, def);
                }
                Err(e) => return usage_error(&format!("--file {e}")),
            },
            "--td" => match value.split_once('=') {
                Some((queue, file)) => {
                    task.td_files.insert(queue.to_ascii_uppercase(), std::path::PathBuf::from(file));
                }
                None => return usage_error("--td needs QUEUE=path"),
            },
            _ => {}
        }
    }
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
    let (mut out, mut err) = (io::stdout().lock(), io::stderr());
    match compiled.execute_cics(library, dds, task, clock, &mut out, &mut err) {
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
        Err(abend) if abend.code == exec::machine::CLOSED_OUTPUT => ExitCode::SUCCESS,
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
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(exec::unit::Clock::Fixed(days * 86_400 + hour * 3600 + minute * 60 + second, hundredths))
}
