//! Each program pins its compiler options on a CBL card, runs its cases, and DISPLAYs every case's
//! storage in hex. The model predicts the same bytes; a run on the real compiler says which of the
//! model's assumptions hold. Generated source uses only EBCDIC-invariant characters, so it means
//! the same bytes whichever code page it is uploaded in.

pub mod families;
pub mod hercules;
pub mod witness;

use numeric::Options;
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub struct Case {
    pub name: String,
    pub assumptions: Vec<&'static str>,
    pub ibm_only: bool,
    /// Level-05 entries of a group that is set up but not dumped. `%` stands for the case number.
    pub operands: Vec<String>,
    /// Level-05 entries of the group whose bytes are dumped.
    pub items: Vec<String>,
    pub statements: Vec<String>,
    pub expect: Vec<u8>,
}

pub struct Program {
    pub name: &'static str,
    pub options: Vec<&'static str>,
    pub cases: Vec<Case>,
}

const AREA_A: &str = "       ";
const AREA_B: &str = "           ";

impl Program {
    pub fn new(name: &'static str, options: &[&'static str], with_national: bool) -> Self {
        assert!(name.len() <= 8);
        let mut parsed = Options::default();
        for option in options {
            assert!(parsed.apply(option).expect("a valid option"), "{option} is not an option the model reads");
        }
        Self { name, options: options.to_vec(), cases: families::all(&parsed, with_national) }
    }

    pub fn case_id(&self, case: &Case) -> String {
        let id = format!("{}.{}", self.name, case.name);
        assert!(id.len() <= 24, "{id} is longer than HEX-ID");
        id
    }

    pub fn source(&self, for_gnucobol: bool) -> String {
        let cases: Vec<(usize, &Case)> = self.cases.iter().filter(|c| !(for_gnucobol && c.ibm_only)).enumerate().map(|(i, c)| (i + 1, c)).collect();
        let fill = |text: &str, n: usize| text.replace('%', &format!("{n:04}"));
        let mut lines: Vec<String> = Vec::new();
        if !for_gnucobol && !self.options.is_empty() {
            lines.push(format!("{AREA_A}CBL {}", self.options.join(",")));
        }
        for text in [
            "IDENTIFICATION DIVISION.",
            &format!("PROGRAM-ID. {}.", self.name),
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  HEX-DIGITS PIC X(16) VALUE '0123456789ABCDEF'.",
            "01  HEX-IN     PIC X(512).",
            "01  HEX-LEN    PIC 9(4) COMP.",
            "01  HEX-I      PIC 9(4) COMP.",
            "01  HEX-J      PIC 9(4) COMP.",
            "01  HEX-N      PIC 9(4) COMP.",
            "01  HEX-HI     PIC 9(4) COMP.",
            "01  HEX-LO     PIC 9(4) COMP.",
            "01  HEX-OFF    PIC 9(4).",
            "01  HEX-LINE   PIC X(64).",
            "01  HEX-ID     PIC X(24).",
            "01  ALL-BYTES  PIC X(256).",
        ] {
            lines.push(format!("{AREA_A}{text}"));
        }
        for &(n, case) in &cases {
            if !case.operands.is_empty() {
                lines.push(fill(&format!("{AREA_A}01  O-%."), n));
                lines.extend(case.operands.iter().map(|o| fill(&format!("{AREA_B}{o}"), n)));
            }
            lines.push(fill(&format!("{AREA_A}01  G-%."), n));
            lines.extend(case.items.iter().map(|o| fill(&format!("{AREA_B}{o}"), n)));
        }
        lines.push(format!("{AREA_A}PROCEDURE DIVISION."));
        lines.push(format!("{AREA_A}MAIN-LINE."));
        lines.push(format!("{AREA_B}PERFORM FILL-ALL-BYTES"));
        lines.extend(cases.iter().map(|&(n, _)| format!("{AREA_B}PERFORM CASE-{n:04}")));
        lines.push(format!("{AREA_B}GOBACK."));
        for &(n, case) in &cases {
            lines.push(format!("{AREA_A}CASE-{n:04}."));
            lines.extend(case.statements.iter().map(|s| fill(&format!("{AREA_B}{s}"), n)));
            lines.push(format!("{AREA_B}MOVE '{}' TO HEX-ID", self.case_id(case)));
            lines.push(fill(&format!("{AREA_B}MOVE G-% TO HEX-IN"), n));
            lines.push(fill(&format!("{AREA_B}MOVE LENGTH OF G-% TO HEX-LEN"), n));
            lines.push(format!("{AREA_B}PERFORM DUMP-CASE."));
        }
        for text in [
            "FILL-ALL-BYTES.",
            "    PERFORM VARYING HEX-I FROM 1 BY 1 UNTIL HEX-I > 256",
            "        MOVE FUNCTION CHAR(HEX-I) TO ALL-BYTES(HEX-I:1)",
            "    END-PERFORM.",
            "DUMP-CASE.",
            "    PERFORM VARYING HEX-I FROM 1 BY 32 UNTIL HEX-I > HEX-LEN",
            "        MOVE SPACES TO HEX-LINE",
            "        PERFORM VARYING HEX-J FROM 0 BY 1",
            "                UNTIL HEX-J > 31 OR HEX-I + HEX-J > HEX-LEN",
            "            COMPUTE HEX-N =",
            "                FUNCTION ORD(HEX-IN(HEX-I + HEX-J:1)) - 1",
            "            DIVIDE HEX-N BY 16 GIVING HEX-HI REMAINDER HEX-LO",
            "            MOVE HEX-DIGITS(HEX-HI + 1:1)",
            "              TO HEX-LINE(HEX-J * 2 + 1:1)",
            "            MOVE HEX-DIGITS(HEX-LO + 1:1)",
            "              TO HEX-LINE(HEX-J * 2 + 2:1)",
            "        END-PERFORM",
            "        COMPUTE HEX-OFF = HEX-I - 1",
            "        DISPLAY 'CASE ' HEX-ID ' ' HEX-OFF ' ' HEX-LINE",
            "    END-PERFORM.",
        ] {
            lines.push(format!("{AREA_A}{text}"));
        }
        for line in &lines {
            assert!(line.len() <= 72, "past column 72: {line}");
            assert!(line.bytes().all(is_invariant), "a code-page-variant character: {line}");
        }
        lines.join("\n") + "\n"
    }

    /// A compile, link and go job. The JOB card's accounting fields are site-specific.
    pub fn jcl(&self) -> String {
        let mut out = String::new();
        writeln!(out, "//{:<8} JOB (ACCT),'IRONWORK FOR COBOL',CLASS=A,MSGCLASS=H,", self.name).unwrap();
        writeln!(out, "//             MSGLEVEL=(1,1),NOTIFY=&SYSUID").unwrap();
        writeln!(out, "//CLG      EXEC IGYWCLG").unwrap();
        writeln!(out, "//COBOL.SYSIN DD *").unwrap();
        out.push_str(&self.source(false));
        writeln!(out, "/*").unwrap();
        writeln!(out, "//GO.SYSOUT DD SYSOUT=*").unwrap();
        out
    }
}

/// EBCDIC's invariant characters: the same byte in every code page this compiler carries.
fn is_invariant(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b" +<=>%&*\"'(),_-./:;?".contains(&b)
}

pub fn programs() -> Vec<Program> {
    vec![
        Program::new("ORAC01", &["TRUNC(STD)", "NUMPROC(NOPFD)", "ARITH(COMPAT)"], true),
        Program::new("ORAC02", &["TRUNC(OPT)", "NUMPROC(PFD)", "ARITH(EXTEND)"], false),
        Program::new("ORAC03", &["TRUNC(BIN)", "NUMPROC(NOPFD)", "ARITH(COMPAT)"], false),
        Program::new("ORAC04", &["TRUNC(OPT)", "NUMPROC(NOPFD)", "ARITH(COMPAT)"], false),
    ]
}

/// Job output: every `CASE <id> <offset> <hex>` line reassembled into each case's bytes, and the
/// compiler named in the listing header, if the output carries one.
pub struct Output {
    pub compiler: Option<String>,
    pub cases: BTreeMap<String, Vec<u8>>,
}

pub fn parse_output(text: &str) -> Output {
    let mut chunks: BTreeMap<String, Vec<(u32, Vec<u8>)>> = BTreeMap::new();
    let compiler = witness::parse_listing(text).compiler;
    for line in text.lines() {
        let Some(at) = line.find("CASE ") else { continue };
        let mut fields = line[at + 5..].split_whitespace();
        let (Some(id), Some(offset), Some(hex)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let (Ok(offset), Some(bytes)) = (offset.parse::<u32>(), unhex(hex)) else { continue };
        chunks.entry(id.to_owned()).or_default().push((offset, bytes));
    }
    let cases = chunks
        .into_iter()
        .map(|(id, mut parts)| {
            parts.sort_by_key(|&(offset, _)| offset);
            (id, parts.into_iter().flat_map(|(_, b)| b).collect())
        })
        .collect();
    Output { compiler, cases }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

pub(crate) fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Match,
    Mismatch { expected: Vec<u8>, observed: Vec<u8> },
    Missing,
}

pub struct Finding {
    pub id: String,
    pub assumptions: Vec<&'static str>,
    pub verdict: Verdict,
}

pub struct Tally {
    pub matched: usize,
    pub mismatched: usize,
    pub missing: usize,
    /// Per assumption id: cases that held it and cases that broke it, among those the output carried.
    pub by_assumption: BTreeMap<&'static str, (usize, usize)>,
}

pub fn tally(findings: &[Finding]) -> Tally {
    let mut tally = Tally { matched: 0, mismatched: 0, missing: 0, by_assumption: BTreeMap::new() };
    for f in findings {
        match f.verdict {
            Verdict::Match => tally.matched += 1,
            Verdict::Mismatch { .. } => tally.mismatched += 1,
            Verdict::Missing => {
                tally.missing += 1;
                continue;
            }
        }
        for &id in &f.assumptions {
            let entry = tally.by_assumption.entry(id).or_default();
            if f.verdict == Verdict::Match { entry.0 += 1 } else { entry.1 += 1 }
        }
    }
    tally
}

pub fn check(programs: &[Program], observed: &BTreeMap<String, Vec<u8>>) -> Vec<Finding> {
    programs
        .iter()
        .flat_map(|p| p.cases.iter().map(move |c| (p, c)))
        .map(|(p, c)| {
            let id = p.case_id(c);
            let verdict = match observed.get(&id) {
                None => Verdict::Missing,
                Some(bytes) if *bytes == c.expect => Verdict::Match,
                Some(bytes) => Verdict::Mismatch { expected: c.expect.clone(), observed: bytes.clone() },
            };
            Finding { id, assumptions: c.assumptions.clone(), verdict }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unhex_rejects_non_ascii_and_odd_text_without_panicking() {
        assert_eq!(unhex("0aFf"), Some(vec![0x0A, 0xFF]));
        assert_eq!(unhex("é1"), None);
        assert_eq!(unhex("1é"), None);
        assert_eq!(unhex("abc"), None);
    }

    #[test]
    fn every_program_renders_within_column_72_in_invariant_characters() {
        for p in programs() {
            assert!(p.source(false).contains("PROGRAM-ID."));
            assert!(p.jcl().starts_with(&format!("//{}", p.name)));
            p.source(true);
        }
    }

    #[test]
    fn case_ids_are_unique() {
        let programs = programs();
        let mut ids: Vec<String> = programs.iter().flat_map(|p| p.cases.iter().map(|c| p.case_id(c))).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn output_lines_reassemble_across_chunks_and_ignore_the_rest() {
        let text = "1PP 5655-EC6 IBM Enterprise COBOL for z/OS  6.4.0 P230418   Date 09/27/2026\n\
                    CASE ORAC01.x.1             0032 CC\n\
                    garbage\n\
                    CASE ORAC01.x.1             0000 AABB\n";
        let out = parse_output(text);
        assert_eq!(out.cases["ORAC01.x.1"], [0xAA, 0xBB, 0xCC]);
        assert_eq!(out.compiler.as_deref(), Some("IBM Enterprise COBOL for z/OS 6.4.0 P230418"));
    }

    #[test]
    fn predictions_score_against_themselves() {
        let programs = programs();
        let observed: BTreeMap<String, Vec<u8>> =
            programs.iter().flat_map(|p| p.cases.iter().map(move |c| (p.case_id(c), c.expect.clone()))).collect();
        assert!(check(&programs, &observed).iter().all(|f| f.verdict == Verdict::Match));
    }
}
