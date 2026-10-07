//! A witness record: what one site's run of the deck showed, in a form that can be published. It
//! names the deck and the compiler, hashes each spool file, and counts the cases and assumptions
//! the target agreed with. The target's bytes stay out of it: a disagreement is named by its case
//! id, and the spool stays with whoever ran the jobs.

use crate::{Tally, check, hex, parse_output, programs, tally};
use numeric::assumptions::{self, Basis};
use rt::digest::sha256;
use rt::evidence::{Value, canonical, fields};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "ironwork-witness/v1";
const DOMAIN: &str = "ironwork-witness/v1\n";

/// The compile listing's header and its options block, as the spool carries them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub compiler: Option<String>,
    pub date: Option<String>,
    pub options: Vec<String>,
}

pub fn parse_listing(text: &str) -> Listing {
    let mut listing = Listing::default();
    let mut in_options = false;
    for raw in text.lines() {
        let line = without_carriage_control(raw);
        if listing.compiler.is_none()
            && let Some(at) = line.find("IBM Enterprise COBOL")
        {
            let mut tokens = line[at..].split_whitespace();
            listing.compiler = Some(tokens.by_ref().take_while(|t| *t != "Date").collect::<Vec<_>>().join(" "));
            listing.date = tokens.next().map(str::to_owned);
        }
        let trimmed = line.trim();
        if in_options {
            let tokens: Vec<&str> = trimmed.split("  ").map(str::trim).filter(|t| !t.is_empty()).collect();
            if tokens.is_empty() || !tokens.iter().all(|t| is_option(t)) {
                in_options = false;
            } else {
                listing.options.extend(tokens.into_iter().map(str::to_owned));
            }
        } else if trimmed.trim_end_matches(':') == "Options in effect" {
            in_options = true;
        }
    }
    listing
}

/// ASA control in column 1: '1' starts a page, '0' and '-' leave blank lines, '+' overprints.
fn without_carriage_control(line: &str) -> &str {
    match line.as_bytes().first() {
        Some(b'1' | b'0' | b'-' | b'+') => &line[1..],
        _ => line,
    }
}

fn is_option(token: &str) -> bool {
    let mut chars = token.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric() || "(),-".contains(c))
}

pub fn basis_name(basis: Basis) -> &'static str {
    match basis {
        Basis::Documented => "documented",
        Basis::Recalled => "recalled",
        Basis::Chosen => "chosen",
        Basis::Observed => "observed",
    }
}

pub fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    Ok(paths)
}

pub struct Witness {
    pub record: BTreeMap<String, Value>,
    pub tally: Tally,
}

pub fn witness(dir: &Path, runner: Option<&str>) -> Result<Witness, String> {
    let mut observed = BTreeMap::new();
    let mut spool = BTreeMap::new();
    let mut compilers = Vec::new();
    let mut dates = Vec::new();
    for path in files(dir)? {
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        observed.extend(parse_output(&text).cases);
        let listing = parse_listing(&text);
        let mut entry = fields([
            ("sha256", hex(&sha256(&bytes)).into()),
            ("bytes", (bytes.len() as u64).into()),
            ("options", strings(&listing.options)),
        ]);
        if let Some(compiler) = listing.compiler {
            entry.insert("compiler".into(), compiler.as_str().into());
            compilers.push(compiler);
        }
        if let Some(date) = listing.date {
            entry.insert("date".into(), date.as_str().into());
            dates.push(date);
        }
        spool.insert(path.file_name().unwrap_or_default().to_string_lossy().into_owned(), Value::Obj(entry));
    }
    if observed.is_empty() {
        return Err(format!("no CASE lines in {}: save each job's whole output, SYSOUT included", dir.display()));
    }
    compilers.sort();
    compilers.dedup();
    dates.sort();
    dates.dedup();

    let findings = check(&programs(), &observed);
    let tally = tally(&findings);
    let disagreed: Vec<Value> = findings.iter().filter(|f| matches!(f.verdict, crate::Verdict::Mismatch { .. })).map(|f| f.id.as_str().into()).collect();
    let held = tally
        .by_assumption
        .iter()
        .map(|(id, (held, broken))| {
            let basis = assumptions::ASSUMPTIONS.iter().find(|a| a.id == *id).map_or("unknown", |a| basis_name(a.basis));
            (id.to_string(), Value::Obj(fields([("held", (*held as u64).into()), ("broken", (*broken as u64).into()), ("basis", basis.into())])))
        })
        .collect();
    let deck = fields([
        ("version", env!("CARGO_PKG_VERSION").into()),
        ("programs", Value::Obj(programs().iter().map(|p| (p.name.to_string(), hex(&sha256(p.source(false).as_bytes())).into())).collect())),
    ]);
    let mut record = fields([
        ("format", FORMAT.into()),
        ("deck", Value::Obj(deck)),
        ("target", Value::Obj(fields([("compilers", strings(&compilers)), ("dates", strings(&dates))]))),
        ("spool", Value::Obj(spool)),
        (
            "cases",
            Value::Obj(fields([
                ("match", (tally.matched as u64).into()),
                ("mismatch", (tally.mismatched as u64).into()),
                ("missing", (tally.missing as u64).into()),
                ("mismatched", Value::Arr(disagreed)),
            ])),
        ),
        ("assumptions", Value::Obj(held)),
    ]);
    if let Some(runner) = runner {
        record.insert("runner".into(), runner.into());
    }
    record.insert("hash".into(), record_hash(&record).into());
    Ok(Witness { record, tally })
}

fn strings(items: &[String]) -> Value {
    Value::Arr(items.iter().map(|s| s.as_str().into()).collect())
}

pub fn record_hash(record: &BTreeMap<String, Value>) -> String {
    let mut body = record.clone();
    body.remove("hash");
    hex(&sha256(format!("{DOMAIN}{}", canonical(&Value::Obj(body))).as_bytes()))
}

pub fn text(record: &BTreeMap<String, Value>) -> String {
    canonical(&Value::Obj(record.clone())) + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "1PP 5655-EC6 IBM Enterprise COBOL for z/OS  6.4.0 P230418            Date 09/27/2026  Time 12:00:00   Page     1\n\
                           0Invocation parameters:\n  LIB\n\
                           0Options in effect:\n      NOADATA\n      ARCH(10)\n      ARITH(COMPAT)\n      TEST(NOEJPD,NOSEPARATE,SOURCE)\n\
                           0\n\
                           1PP 5655-EC6 IBM Enterprise COBOL for z/OS  6.4.0 P230418  ORAC01    Date 09/27/2026  Time 12:00:00   Page     2\n\
                           0 LineID  PL SL  ----+-*A-1-B--+----2\n";

    #[test]
    fn listing_header_and_options_are_read_through_carriage_control() {
        let listing = parse_listing(LISTING);
        assert_eq!(listing.compiler.as_deref(), Some("IBM Enterprise COBOL for z/OS 6.4.0 P230418"));
        assert_eq!(listing.date.as_deref(), Some("09/27/2026"));
        assert_eq!(listing.options, ["NOADATA", "ARCH(10)", "ARITH(COMPAT)", "TEST(NOEJPD,NOSEPARATE,SOURCE)"]);
        assert_eq!(parse_listing("CASE ORAC01.x 0000 AA\n"), Listing::default());
    }

    fn spool_dir(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ironwork-witness-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ORAC01.txt"), text).unwrap();
        dir
    }

    #[test]
    fn record_names_a_disagreement_without_its_bytes() {
        let programs = programs();
        let (program, case) = (&programs[0], &programs[0].cases[0]);
        let mut wrong = case.expect.clone();
        wrong[0] ^= 0xFF;
        let dir = spool_dir("mismatch", &format!("{LISTING}CASE {} 0000 {}\n", program.case_id(case), hex(&wrong)));
        let witness = witness(&dir, Some("a test site")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let json = text(&witness.record);
        assert_eq!((witness.tally.matched, witness.tally.mismatched), (0, 1));
        assert!(json.contains(&format!("\"mismatched\":[\"{}\"]", program.case_id(case))));
        assert!(!json.contains(&hex(&wrong)));
        assert!(!json.contains(&hex(&case.expect)));
        assert!(!json.contains("CASE "));
        assert!(json.contains("\"runner\":\"a test site\""));
        assert!(json.contains("\"compilers\":[\"IBM Enterprise COBOL for z/OS 6.4.0 P230418\"]"));
        assert!(json.contains("\"options\":[\"NOADATA\",\"ARCH(10)\",\"ARITH(COMPAT)\",\"TEST(NOEJPD,NOSEPARATE,SOURCE)\"]"));
        assert!(json.contains("\"basis\":\""));
        let Value::Str(hash) = &witness.record["hash"] else { panic!("hash is a string") };
        assert_eq!(hash, &record_hash(&witness.record));
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn a_matching_run_counts_each_assumption_as_held_and_omits_an_absent_runner() {
        let programs = programs();
        let (program, case) = (&programs[0], &programs[0].cases[0]);
        let dir = spool_dir("match", &format!("CASE {} 0000 {}\n", program.case_id(case), hex(&case.expect)));
        let witness = witness(&dir, None).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let json = text(&witness.record);
        assert_eq!((witness.tally.matched, witness.tally.mismatched), (1, 0));
        assert!(json.contains("\"mismatched\":[]"));
        assert!(!json.contains("\"runner\""));
        assert!(json.contains("\"compilers\":[]"));
        for id in &case.assumptions {
            assert!(json.contains(&format!("\"{id}\":{{\"basis\":\"")), "{id} is tallied");
        }
    }

    #[test]
    fn spool_without_cases_is_refused() {
        let dir = spool_dir("empty", LISTING);
        let result = witness(&dir, None);
        fs::remove_dir_all(&dir).unwrap();
        assert!(result.is_err_and(|e| e.contains("no CASE lines")));
    }
}
