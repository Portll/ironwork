use numeric::assumptions::{self, Basis, Oracle};
use ironwork_oracle::hercules::{self, Case, Op, Outcome};
use ironwork_oracle::{Finding, Verdict, check, hex, parse_output, programs};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

const USAGE: &str = "ironwork for COBOL: the conformance oracle
usage:
  ironwork-oracle generate <dir>   write each program's COBOL source, its JCL, and expected.tsv
  ironwork-oracle check <dir>      score job output saved in <dir> against the predictions
  ironwork-oracle smoke <dir>      compile and run the programs with GnuCOBOL (a syntax check, not an oracle)
  ironwork-oracle hercules <dir>   run the decimal and HFP cases under Hercules and compare with the model";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["generate", dir] => generate(Path::new(dir)),
        ["check", dir] => score(Path::new(dir)),
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

fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    Ok(paths)
}

fn report(findings: &[Finding]) -> bool {
    let mut by_assumption: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let (mut matched, mut mismatched, mut missing) = (0, 0, 0);
    for f in findings {
        match &f.verdict {
            Verdict::Match => matched += 1,
            Verdict::Missing => missing += 1,
            Verdict::Mismatch { expected, observed } => {
                mismatched += 1;
                println!("MISMATCH {}\n  expected {}\n  observed {}", f.id, hex(expected), hex(observed));
            }
        }
        if f.verdict == Verdict::Missing {
            continue;
        }
        for &id in &f.assumptions {
            let entry = by_assumption.entry(id).or_default();
            if f.verdict == Verdict::Match { entry.0 += 1 } else { entry.1 += 1 }
        }
    }
    println!("\n{matched} match, {mismatched} mismatch, {missing} not in the output\n");
    println!("assumption  held  broken  basis     settled by        claim");
    for a in assumptions::ASSUMPTIONS {
        let (held, broken) = by_assumption.get(a.id).copied().unwrap_or_default();
        let basis = match a.basis {
            Basis::Documented => "documented",
            Basis::Recalled => "recalled",
            Basis::Chosen => "chosen",
        };
        let oracle = match a.oracle {
            Oracle::Hercules => "Hercules",
            Oracle::EnterpriseCobol => "Enterprise COBOL",
        };
        println!("{:<11} {held:>4}  {broken:>6}  {basis:<9} {oracle:<17} {}", a.id, a.claim);
    }
    mismatched == 0
}

fn smoke(dir: &Path) -> Result<bool, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut observed = BTreeMap::new();
    for p in programs() {
        let source = dir.join(format!("{}.cbl", p.name));
        let binary = dir.join(p.name);
        write(&source, &p.source(true))?;
        let compile = Command::new("cobc").args(["-x", "-std=ibm", "-o"]).arg(&binary).arg(&source).output().map_err(|e| format!("cobc: {e}"))?;
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
