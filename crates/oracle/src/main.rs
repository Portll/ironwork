use numeric::assumptions::{self, Oracle};
use ironwork_oracle::hercules::{self, Case, Op, Outcome};
use ironwork_oracle::witness::{self, basis_name, files};
use ironwork_oracle::{Finding, Verdict, check, hex, parse_output, programs, tally};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

const USAGE: &str = "ironwork for COBOL: the conformance oracle
usage:
  ironwork-oracle generate <dir>   write each program's COBOL source, its JCL, and expected.tsv
  ironwork-oracle check <dir>      score job output saved in <dir> against the predictions
  ironwork-oracle witness <dir> [--runner TEXT] [--out FILE]
                                   write the publishable record of a run: the deck, the compiler and its
                                   options, each spool file's hash and the scores, never the compiler's bytes
  ironwork-oracle smoke <dir>      compile and run the programs with GnuCOBOL (a syntax check, not an oracle)
  ironwork-oracle hercules <dir>   run the decimal and HFP cases under Hercules and compare with the model";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["generate", dir] => generate(Path::new(dir)),
        ["check", dir] => score(Path::new(dir)),
        ["witness", dir, rest @ ..] => witness_record(Path::new(dir), rest),
        ["smoke", dir] => smoke(Path::new(dir)),
        ["hercules", dir] => hercules_run(Path::new(dir)),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn generate(dir: &Path) -> Result<bool, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut tsv = String::from("id\tassumptions\texpected\n");
    for p in programs() {
        write(&dir.join(format!("{}.cbl", p.name)), &p.source(false))?;
        write(&dir.join(format!("{}.jcl", p.name)), &p.jcl())?;
        for c in &p.cases {
            tsv.push_str(&format!("{}\t{}\t{}\n", p.case_id(c), c.assumptions.join(","), hex(&c.expect)));
        }
    }
    write(&dir.join("expected.tsv"), &tsv)?;
    println!("wrote {} programs to {}", programs().len(), dir.display());
    Ok(true)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn score(dir: &Path) -> Result<bool, String> {
    let mut observed = BTreeMap::new();
    let mut compilers = Vec::new();
    for path in files(dir)? {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let output = parse_output(&text);
        compilers.extend(output.compiler);
        observed.extend(output.cases);
    }
    compilers.sort();
    compilers.dedup();
    println!("target: {}", if compilers.is_empty() { "not named in the output".into() } else { compilers.join("; ") });
    Ok(report(&check(&programs(), &observed)))
}

fn witness_record(dir: &Path, rest: &[&str]) -> Result<bool, String> {
    let (mut runner, mut out) = (None, None);
    let mut args = rest.iter();
    while let Some(flag) = args.next() {
        match (*flag, args.next()) {
            ("--runner", Some(text)) => runner = Some(*text),
            ("--out", Some(path)) => out = Some(PathBuf::from(path)),
            _ => return Err(USAGE.into()),
        }
    }
    let witness = witness::witness(dir, runner)?;
    let text = witness::text(&witness.record);
    let t = &witness.tally;
    match out {
        Some(path) => {
            write(&path, &text)?;
            println!("wrote {}: {} match, {} mismatch, {} not in the output", path.display(), t.matched, t.mismatched, t.missing);
        }
        None => print!("{text}"),
    }
    Ok(true)
}

fn report(findings: &[Finding]) -> bool {
    for f in findings {
        if let Verdict::Mismatch { expected, observed } = &f.verdict {
            println!("MISMATCH {}\n  expected {}\n  observed {}", f.id, hex(expected), hex(observed));
        }
    }
    let t = tally(findings);
    println!("\n{} match, {} mismatch, {} not in the output\n", t.matched, t.mismatched, t.missing);
    println!("assumption  held  broken  basis     settled by        claim");
    for a in assumptions::ASSUMPTIONS {
        let (held, broken) = t.by_assumption.get(a.id).copied().unwrap_or_default();
        let oracle = match a.oracle {
            Oracle::Hercules => "Hercules",
            Oracle::EnterpriseCobol => "Enterprise COBOL",
            Oracle::Db2 => "Db2 for z/OS",
        };
        println!("{:<11} {held:>4}  {broken:>6}  {:<9} {oracle:<17} {}", a.id, basis_name(a.basis), a.claim);
    }
    t.mismatched == 0
}

fn smoke(dir: &Path) -> Result<bool, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut observed = BTreeMap::new();
    for p in programs() {
        let source = dir.join(format!("{}.cbl", p.name));
        let binary = dir.join(p.name);
        write(&source, &p.source(true))?;
        let compile = Command::new("cobc").args(["-x", "-std=ibm-strict", "-o"]).arg(&binary).arg(&source).output().map_err(|e| format!("cobc: {e}"))?;
        if !compile.status.success() {
            return Err(format!("{} does not compile:\n{}", p.name, String::from_utf8_lossy(&compile.stderr)));
        }
        let run = Command::new(&binary).output().map_err(|e| format!("{}: {e}", binary.display()))?;
        observed.extend(parse_output(&String::from_utf8_lossy(&run.stdout)).cases);
    }
    println!("GnuCOBOL is ASCII and IEEE: its results are not IBM's, and this is no oracle.");
    let findings: Vec<Finding> = check(&programs(), &observed).into_iter().filter(|f| f.verdict != Verdict::Missing).collect();
    let agree = findings.iter().filter(|f| f.verdict == Verdict::Match).count();
    println!("{} programs compiled and ran; {agree} of {} cases agree with the model", programs().len(), findings.len());
    Ok(true)
}

fn hercules_run(dir: &Path) -> Result<bool, String> {
    let results = hercules::run(dir, "hercules")?;
    println!("{} against ironwork {}\n", hercules_version("hercules"), env!("CARGO_PKG_VERSION"));
    let mut by_instruction: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut disagreements = Vec::new();
    for (case, seen, expected) in &results {
        let entry = by_instruction.entry(case.instruction()).or_default();
        if hercules::agrees(seen, expected) {
            entry.0 += 1;
        } else {
            entry.1 += 1;
            disagreements.push((case, seen, expected));
        }
    }
    println!("instruction  agree  disagree");
    for (name, (agree, disagree)) in &by_instruction {
        println!("{name:<12} {agree:>5}  {disagree:>8}");
    }
    for (case, seen, expected) in &disagreements {
        println!("\nDISAGREE {}\n  operands   {}\n  hercules   {}\n  zarch      {}", case.name, operands(case), outcome(seen), outcome(expected));
    }
    println!("\n{} agree, {} disagree", results.len() - disagreements.len(), disagreements.len());
    Ok(disagreements.is_empty())
}

fn hercules_version(hercules: &str) -> String {
    let said = Command::new(hercules).arg("--version").output().map(|o| [o.stdout, o.stderr].concat()).unwrap_or_default();
    let text = String::from_utf8_lossy(&said);
    match text.split_once("Hercules version ") {
        Some((_, rest)) => format!("Hercules {}", rest.split_whitespace().next().unwrap_or("")),
        None => "Hercules, version not reported".into(),
    }
}

fn operands(case: &Case) -> String {
    match &case.op {
        Op::Pack { op1_len, op2 } | Op::Unpk { op1_len, op2 } => format!("op1_len={op1_len} op2={}", hex(op2)),
        Op::Zap { op1, op2 } | Op::Ap { op1, op2 } | Op::Sp { op1, op2 } | Op::Mp { op1, op2 } | Op::Dp { op1, op2 } | Op::Cp { op1, op2 } => {
            format!("op1={} op2={}", hex(op1), hex(op2))
        }
        Op::Srp { op1, shift, rounding } => format!("op1={} shift={shift:#04x} rounding={rounding}", hex(op1)),
        Op::Tp { op } => format!("op={}", hex(op)),
        Op::Cvb { op2 } => format!("op2={}", hex(op2)),
        Op::Cvd { value } => format!("value={value}"),
        Op::Hfp { op1, op2, mask, .. } => format!("op1={} op2={} mask={mask:X}", hex(op1), hex(op2)),
    }
}

fn outcome(o: &Outcome) -> String {
    let cc = o.cc.map_or("-".to_owned(), |c| c.to_string());
    let interruption = o.interruption.map_or("none".to_owned(), |c| format!("{c:04X}"));
    format!("result={} cc={cc} interruption={interruption}", if o.result.is_empty() { "-".into() } else { hex(&o.result) })
}
