//! The CICS system definition as DFHCSDUP reads it: DEFINE commands naming resources, with
//! KEYWORD(value) attributes running on to the next command.

use crate::messages::Message;
use crate::{Error, Pos};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Csd {
    pub transactions: BTreeMap<String, Transaction>,
    pub programs: BTreeMap<String, Program>,
    pub files: BTreeMap<String, File>,
    pub tdqueues: BTreeMap<String, TdQueue>,
    pub urimaps: BTreeMap<String, Urimap>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub program: Option<String>,
    pub group: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub group: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub dsname: Option<String>,
    pub group: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TdQueue {
    pub kind: Option<String>,
    pub ddname: Option<String>,
    pub indirect: Option<String>,
    pub group: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Urimap {
    pub usage: Option<String>,
    pub path: Option<String>,
    pub program: Option<String>,
    pub transaction: Option<String>,
    pub group: Option<String>,
    pub line: u32,
}

/// Only DEFINE commands of the kinds above are kept, and a later definition of a kind and name
/// replaces an earlier one.
pub fn parse(text: &str) -> Result<Csd, Error> {
    let mut csd = Csd::default();
    for (line, block) in blocks(text) {
        if !starts_with_word(&block, "DEFINE") {
            continue;
        }
        if block.bytes().filter(|&b| b == b'(').count() > block.bytes().filter(|&b| b == b')').count() {
            return Err(fail(line, crate::messages::IWP0040, "unbalanced '(' in DEFINE"));
        }
        let Some((kind, name, rest)) = parse_head(&block) else {
            return Err(fail(line, crate::messages::IWP0041, "DEFINE names no KIND(NAME)"));
        };
        let limit = match kind.as_str() {
            "TRANSACTION" => 4,
            "PROGRAM" | "FILE" | "TDQUEUE" | "URIMAP" => 8,
            _ => continue,
        };
        if name.len() > limit {
            return Err(fail(line, crate::messages::IWP0042, format!("{kind}({name}): a {kind} name is at most {limit} characters")));
        }
        let attrs = parse_attrs(rest, line)?;
        let get = |keyword: &str| attrs.get(keyword).cloned();
        match kind.as_str() {
            "TRANSACTION" => {
                csd.transactions.insert(name, Transaction { program: get("PROGRAM"), group: get("GROUP"), line });
            }
            "PROGRAM" => {
                csd.programs.insert(name, Program { group: get("GROUP"), line });
            }
            "FILE" => {
                csd.files.insert(name, File { dsname: get("DSNAME"), group: get("GROUP"), line });
            }
            "TDQUEUE" => {
                csd.tdqueues.insert(name, TdQueue { kind: get("TYPE"), ddname: get("DDNAME"), indirect: get("INDIRECTNAME"), group: get("GROUP"), line });
            }
            _ => {
                let urimap = Urimap { usage: get("USAGE"), path: get("PATH"), program: get("PROGRAM"), transaction: get("TRANSACTION"), group: get("GROUP"), line };
                csd.urimaps.insert(name, urimap);
            }
        }
    }
    Ok(csd)
}

impl Csd {
    /// The DD an extrapartition queue is written to, following one INDIRECT hop; None for an
    /// intrapartition queue or one nobody defined.
    pub fn dd_of_queue(&self, queue: &str) -> Option<&str> {
        let q = self.tdqueues.get(queue)?;
        match q.kind.as_deref() {
            Some("EXTRA") => q.ddname.as_deref(),
            Some("INDIRECT") => {
                let target = q.indirect.as_deref()?;
                let target_q = self.tdqueues.get(target)?;
                if target_q.kind.as_deref() == Some("EXTRA") {
                    target_q.ddname.as_deref()
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn fail(line: u32, message: Message, text: impl Into<String>) -> Error {
    message.at(Pos { file: 0, line, col: 1 }, text)
}

const COMMANDS: [&str; 14] = ["DEFINE", "ADD", "ALTER", "APPEND", "COPY", "DELETE", "INITIALIZE", "LIST", "REMOVE", "SCAN", "SERVICE", "UPGRADE", "USERDEFINE", "VERIFY"];

/// Whether `text`, after leading blanks, starts with `word` as a whole word, in any case.
fn starts_with_word(text: &str, word: &str) -> bool {
    let text = text.trim_start().as_bytes();
    text.len() >= word.len() && text[..word.len()].eq_ignore_ascii_case(word.as_bytes()) && !text.get(word.len()).is_some_and(u8::is_ascii_alphanumeric)
}

/// Each command with the 1-based line it starts on: comment lines dropped, and the lines after a
/// command joined to it until the next one starts.
fn blocks(text: &str) -> Vec<(u32, String)> {
    let mut blocks: Vec<(u32, String)> = Vec::new();
    for (line, n) in text.lines().zip(1u32..) {
        if line.trim_start().starts_with('*') {
            continue;
        }
        if COMMANDS.iter().any(|c| starts_with_word(line, c)) {
            blocks.push((n, line.to_owned()));
        } else if let Some((_, block)) = blocks.last_mut() {
            block.push(' ');
            block.push_str(line);
        }
    }
    blocks
}

fn parse_head(text: &str) -> Option<(String, String, &str)> {
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    if !text[i..].to_ascii_uppercase().starts_with("DEFINE") {
        return None;
    }
    i += 6;

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    let kind_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        i += 1;
    }
    let kind = text[kind_start..i].to_uppercase();

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    if i >= bytes.len() || bytes[i] != b'(' {
        return None;
    }
    i += 1;

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    let name_start = i;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b')' {
        i += 1;
    }
    let name = text[name_start..i].to_uppercase();
    if kind.is_empty() || name.is_empty() {
        return None;
    }

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    if i >= bytes.len() || bytes[i] != b')' {
        return None;
    }
    i += 1;

    Some((kind, name, &text[i..]))
}

fn parse_attrs(rest: &str, line: u32) -> Result<BTreeMap<String, String>, Error> {
    let mut attrs = BTreeMap::new();
    let bytes = rest.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let key_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
            i += 1;
        }
        let key = &rest[key_start..i];
        if key.is_empty() {
            i += 1;
            continue;
        }

        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= bytes.len() || bytes[i] != b'(' {
            continue;
        }
        i += 1;

        let val_start = i;
        let mut val_len = 0;
        while i < bytes.len() && bytes[i] != b')' {
            i += 1;
            val_len += 1;
            if val_len > 256 {
                return Err(fail(
                    line,
                    crate::messages::IWP0043, format!("attribute value for '{}' exceeds 256 characters", key),
                ));
            }
        }

        if i >= bytes.len() {
            return Err(fail(line, crate::messages::IWP0044, "unbalanced '(' in attribute value"));
        }
        i += 1;

        let val = rest[val_start..i - 1].trim();
        let val = if key.eq_ignore_ascii_case("PATH") {
            val.to_string()
        } else {
            val.to_uppercase()
        };

        attrs.insert(key.to_uppercase(), val);
    }

    Ok(attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_definitions() {
        let input = "DEFINE TRANSACTION(CECI) GROUP(LOCAL)\n       PROGRAM(DFHECIP) TASKDATALOC(ANY)\n DEFINE TRANSACTION(PAY1) GROUP(PAYROLL)\n        PROGRAM(PAYADM) CMDSEC(NO) RESSEC(NO)\n DEFINE TDQUEUE(JOBS) GROUP(APP)\n        DESCRIPTION(SUBMIT JOBS FROM CICS)\n        TYPE(EXTRA) DDNAME(INREADER)\n DEFINE TDQUEUE(LOGQ) GROUP(APP) TYPE(INDIRECT) INDIRECTNAME(JOBS)\n DEFINE PROGRAM(PAYADM) GROUP(PAYROLL) LANGUAGE(COBOL)\n DEFINE FILE(ACCTDAT) GROUP(APP) DSNAME(AWS.M2.CARDDEMO.ACCTDATA.VSAM.KSDS)\n DEFINE URIMAP(ECHO) GROUP(WEB) USAGE(SERVER) PATH(/cics/echo) PROGRAM(WEBECHO)\n ADD GROUP(PAYROLL) LIST(REGLIST)";
        let csd = parse(input).unwrap();

        assert_eq!(csd.transactions.len(), 2);
        let ceci = csd.transactions.get("CECI").unwrap();
        assert_eq!(ceci.program.as_deref(), Some("DFHECIP"));
        assert_eq!(ceci.group.as_deref(), Some("LOCAL"));
        assert_eq!(ceci.line, 1);

        let pay1 = csd.transactions.get("PAY1").unwrap();
        assert_eq!(pay1.program.as_deref(), Some("PAYADM"));
        assert_eq!(pay1.group.as_deref(), Some("PAYROLL"));
        assert_eq!(pay1.line, 3);

        assert_eq!(csd.tdqueues.len(), 2);
        let jobs = csd.tdqueues.get("JOBS").unwrap();
        assert_eq!(jobs.kind.as_deref(), Some("EXTRA"));
        assert_eq!(jobs.ddname.as_deref(), Some("INREADER"));
        assert_eq!(jobs.indirect, None);
        assert_eq!(jobs.group.as_deref(), Some("APP"));
        assert_eq!(jobs.line, 5);

        let logq = csd.tdqueues.get("LOGQ").unwrap();
        assert_eq!(logq.kind.as_deref(), Some("INDIRECT"));
        assert_eq!(logq.indirect.as_deref(), Some("JOBS"));
        assert_eq!(logq.ddname, None);
        assert_eq!(logq.line, 8);

        assert_eq!(csd.programs.len(), 1);
        let payadm = csd.programs.get("PAYADM").unwrap();
        assert_eq!(payadm.group.as_deref(), Some("PAYROLL"));
        assert_eq!(payadm.line, 9);

        assert_eq!(csd.files.len(), 1);
        let acct = csd.files.get("ACCTDAT").unwrap();
        assert_eq!(acct.dsname.as_deref(), Some("AWS.M2.CARDDEMO.ACCTDATA.VSAM.KSDS"));
        assert_eq!(acct.group.as_deref(), Some("APP"));
        assert_eq!(acct.line, 10);

        assert_eq!(csd.urimaps.len(), 1);
        let echo = csd.urimaps.get("ECHO").unwrap();
        assert_eq!(echo.usage.as_deref(), Some("SERVER"));
        assert_eq!(echo.path.as_deref(), Some("/cics/echo"));
        assert_eq!(echo.program.as_deref(), Some("WEBECHO"));
        assert_eq!(echo.group.as_deref(), Some("WEB"));
        assert_eq!(echo.line, 11);
    }

    #[test]
    fn continuation_lines_and_blanks() {
        let input = "   DEFINE TRANSACTION(CECI)\n   GROUP(LOCAL)\n   PROGRAM(DFHECIP)";
        let csd = parse(input).unwrap();
        let t = csd.transactions.get("CECI").unwrap();
        assert_eq!(t.program.as_deref(), Some("DFHECIP"));
        assert_eq!(t.group.as_deref(), Some("LOCAL"));
        assert_eq!(t.line, 1);
    }

    #[test]
    fn comments_and_non_define_skipped() {
        let input = "* comment\nDEFINE TRANSACTION(CECI) GROUP(LOCAL)\nADD GROUP(LOCAL)\nLIST ALL\nDEFINE TRANSACTION(PAY1) GROUP(PAYROLL)";
        let csd = parse(input).unwrap();
        assert_eq!(csd.transactions.len(), 2);
        assert!(csd.transactions.contains_key("CECI"));
        assert!(csd.transactions.contains_key("PAY1"));
    }

    #[test]
    fn later_define_replaces_earlier() {
        let input = "DEFINE TRANSACTION(CECI) GROUP(LOCAL)\nDEFINE TRANSACTION(CECI) GROUP(NEW)";
        let csd = parse(input).unwrap();
        assert_eq!(csd.transactions.len(), 1);
        let t = csd.transactions.get("CECI").unwrap();
        assert_eq!(t.group.as_deref(), Some("NEW"));
        assert_eq!(t.line, 2);
    }

    #[test]
    fn path_keeps_case() {
        let input = "DEFINE URIMAP(ECHO) PATH(/Cics/Echo)";
        let csd = parse(input).unwrap();
        let u = csd.urimaps.get("ECHO").unwrap();
        assert_eq!(u.path.as_deref(), Some("/Cics/Echo"));
    }

    #[test]
    fn dd_of_queue_extra() {
        let input = "DEFINE TDQUEUE(JOBS) TYPE(EXTRA) DDNAME(INREADER)";
        let csd = parse(input).unwrap();
        assert_eq!(csd.dd_of_queue("JOBS"), Some("INREADER"));
    }

    #[test]
    fn dd_of_queue_indirect() {
        let input = "DEFINE TDQUEUE(JOBS) TYPE(EXTRA) DDNAME(INREADER)\nDEFINE TDQUEUE(LOGQ) TYPE(INDIRECT) INDIRECTNAME(JOBS)";
        let csd = parse(input).unwrap();
        assert_eq!(csd.dd_of_queue("LOGQ"), Some("INREADER"));
    }

    #[test]
    fn dd_of_queue_intra() {
        let input = "DEFINE TDQUEUE(JOBS) TYPE(INTRA)";
        let csd = parse(input).unwrap();
        assert_eq!(csd.dd_of_queue("JOBS"), None);
    }

    #[test]
    fn dd_of_queue_indirect_to_intra() {
        let input = "DEFINE TDQUEUE(JOBS) TYPE(INTRA)\nDEFINE TDQUEUE(LOGQ) TYPE(INDIRECT) INDIRECTNAME(JOBS)";
        let csd = parse(input).unwrap();
        assert_eq!(csd.dd_of_queue("LOGQ"), None);
    }

    #[test]
    fn error_no_kind_name() {
        let input = "DEFINE TRANSACTION";
        let err = parse(input).unwrap_err();
        assert_eq!(err.pos.line, 1);
        assert!(err.message.contains("names no KIND(NAME)"));
    }

    #[test]
    fn error_name_too_long_transaction() {
        let input = "DEFINE TRANSACTION(CECIX) GROUP(LOCAL)";
        let err = parse(input).unwrap_err();
        assert_eq!(err.pos.line, 1);
        assert!(err.message.contains("at most 4 characters"));
    }

    #[test]
    fn error_name_too_long_program() {
        let input = "DEFINE PROGRAM(PAYADMINX) GROUP(LOCAL)";
        let err = parse(input).unwrap_err();
        assert_eq!(err.pos.line, 1);
        assert!(err.message.contains("at most 8 characters"));
    }

    #[test]
    fn skip_unknown_kind_long_name() {
        let input = "DEFINE TCPIPSERVICE(SECUREWEB) GROUP(LOCAL)";
        let csd = parse(input).unwrap();
        assert!(csd.transactions.is_empty());
        assert!(csd.programs.is_empty());
        assert!(csd.files.is_empty());
        assert!(csd.tdqueues.is_empty());
        assert!(csd.urimaps.is_empty());
    }

    #[test]
    fn error_value_too_long() {
        let long_val = "A".repeat(300);
        let input = format!("DEFINE TRANSACTION(CECI) GROUP({})", long_val);
        let err = parse(&input).unwrap_err();
        assert_eq!(err.pos.line, 1);
        assert!(err.message.contains("exceeds 256 characters"));
    }

    #[test]
    fn crlf_same_as_lf() {
        let lf_input = "DEFINE TRANSACTION(CECI) GROUP(LOCAL)\nPROGRAM(DFHECIP)";
        let crlf_input = "DEFINE TRANSACTION(CECI) GROUP(LOCAL)\r\nPROGRAM(DFHECIP)";
        let csd_lf = parse(lf_input).unwrap();
        let csd_crlf = parse(crlf_input).unwrap();
        assert_eq!(csd_lf, csd_crlf);
    }

    #[test]
    fn unbalanced_paren() {
        let input = "DEFINE TRANSACTION(CECI GROUP(LOCAL)";
        let err = parse(input).unwrap_err();
        assert_eq!(err.pos.line, 1);
        assert!(err.message.contains("unbalanced '('"));
    }
}
