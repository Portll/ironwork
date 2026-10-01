//! EXEC DLI commands as the IMS translator reads them (IMS Application Programming: EXEC DLI
//! Commands for CICS and IMS, SC18-7811-04, chapters 4-6): each command, the longer spellings it
//! has, and the options it takes.

pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    /// `None` where the book leaves the options to IMS's Operations Guide: GMSG, ICMD and RCMD.
    pub options: Option<&'static [&'static str]>,
}

const GET_UNIQUE: &[&str] = &[
    "PCB", "AIB", "KEYFEEDBACK", "FEEDBACKLEN", "INTO", "VARIABLE", "LAST", "SEGMENT", "SEGLENGTH", "OFFSET", "LOCKED", "LOCKCLASS", "MOVENEXT", "GETFIRST", "SET",
    "SETCOND", "SETZERO", "SETPARENT", "WHERE", "FIELDLENGTH", "KEYS", "KEYLENGTH",
];

const GET_NEXT: &[&str] = &[
    "PCB", "AIB", "KEYFEEDBACK", "FEEDBACKLEN", "INTO", "VARIABLE", "FIRST", "LAST", "CURRENT", "SEGMENT", "SEGLENGTH", "OFFSET", "LOCKED", "LOCKCLASS", "MOVENEXT",
    "GETFIRST", "SET", "SETCOND", "SETZERO", "SETPARENT", "WHERE", "FIELDLENGTH", "KEYS", "KEYLENGTH",
];

/// From the book's Format diagrams (pp. 35-81). GHU, GHN and GHNP take their Get command's
/// options (p. 102); every command that takes PCB may take AIB instead (p. 5).
pub const COMMANDS: &[Command] = &[
    Command { name: "GU", aliases: &["GET UNIQUE"], options: Some(GET_UNIQUE) },
    Command { name: "GHU", aliases: &[], options: Some(GET_UNIQUE) },
    Command { name: "GN", aliases: &["GET NEXT"], options: Some(GET_NEXT) },
    Command { name: "GHN", aliases: &[], options: Some(GET_NEXT) },
    Command { name: "GNP", aliases: &["GET NEXT IN PARENT"], options: Some(GET_NEXT) },
    Command { name: "GHNP", aliases: &[], options: Some(GET_NEXT) },
    Command { name: "ISRT", aliases: &["INSERT"], options: Some(&["PCB", "AIB", "VARIABLE", "FIRST", "LAST", "CURRENT", "SEGMENT", "SEGLENGTH", "FROM", "OFFSET", "MOVENEXT", "GETFIRST", "SET", "SETCOND", "SETZERO", "WHERE", "FIELDLENGTH", "KEYS", "KEYLENGTH"]) },
    Command { name: "DLET", aliases: &["DELETE"], options: Some(&["PCB", "AIB", "VARIABLE", "SEGMENT", "SEGLENGTH", "FROM", "SETZERO"]) },
    Command { name: "REPL", aliases: &["REPLACE"], options: Some(&["PCB", "AIB", "VARIABLE", "SEGMENT", "SEGLENGTH", "OFFSET", "FROM", "MOVENEXT", "SET", "SETCOND", "SETZERO"]) },
    Command { name: "POS", aliases: &["POSITION"], options: Some(&["PCB", "AIB", "INTO", "KEYFEEDBACK", "FEEDBACKLEN", "SEGMENT", "WHERE", "FIELDLENGTH"]) },
    Command { name: "RETRIEVE", aliases: &[], options: Some(&["PCB", "AIB", "KEYFEEDBACK", "FEEDBACKLEN"]) },
    Command { name: "LOAD", aliases: &[], options: Some(&["PCB", "AIB", "VARIABLE", "SEGMENT", "SEGLENGTH", "FROM"]) },
    Command { name: "SCHD", aliases: &["SCHEDULE"], options: Some(&["PSB", "SYSSERVE", "NODHABEND"]) },
    Command { name: "TERM", aliases: &["TERMINATE"], options: Some(&[]) },
    Command { name: "ACCEPT", aliases: &[], options: Some(&["STATUSGROUP", "AIB"]) },
    Command { name: "CHKP", aliases: &["CHECKPOINT"], options: Some(&["ID", "AIB"]) },
    Command { name: "DEQ", aliases: &[], options: Some(&["LOCKCLASS", "AIB"]) },
    Command { name: "LOG", aliases: &[], options: Some(&["FROM", "LENGTH", "AIB"]) },
    Command { name: "QUERY", aliases: &[], options: Some(&["PCB", "AIB"]) },
    Command { name: "REFRESH", aliases: &[], options: Some(&["DBQUERY", "AIB"]) },
    Command { name: "ROLB", aliases: &[], options: Some(&[]) },
    Command { name: "ROLL", aliases: &[], options: Some(&[]) },
    Command { name: "ROLS", aliases: &[], options: Some(&["PCB", "TOKEN", "AREA", "AIB"]) },
    Command { name: "SETS", aliases: &[], options: Some(&["TOKEN", "AREA", "AIB"]) },
    Command { name: "SETU", aliases: &[], options: Some(&["TOKEN", "AREA"]) },
    Command { name: "STAT", aliases: &["STATISTICS"], options: Some(&["PCB", "INTO", "LENGTH", "VSAM", "NONVSAM", "FORMATTED", "UNFORMATTED", "SUMMARY", "AIB"]) },
    Command { name: "SYMCHKP", aliases: &[], options: Some(&["ID", "AREA1", "AREA2", "AREA3", "AREA4", "AREA5", "AREA6", "AREA7", "LENGTH1", "LENGTH2", "LENGTH3", "LENGTH4", "LENGTH5", "LENGTH6", "LENGTH7"]) },
    Command { name: "XRST", aliases: &[], options: Some(&["MAXLENGTH", "ID", "AREA1", "AREA2", "AREA3", "AREA4", "AREA5", "AREA6", "AREA7", "LENGTH1", "LENGTH2", "LENGTH3", "LENGTH4", "LENGTH5", "LENGTH6", "LENGTH7"]) },
    Command { name: "GMSG", aliases: &[], options: None },
    Command { name: "ICMD", aliases: &[], options: None },
    Command { name: "RCMD", aliases: &[], options: None },
];

/// Options whose argument is a name rather than data: in double parentheses it is an area that
/// holds the name.
pub const NAMED: &[&str] = &["SEGMENT", "PSB"];

/// The command a block's words begin with, longest spelling first, and how many words it takes.
pub fn command(words: &[&str]) -> Option<(&'static Command, usize)> {
    let upper: Vec<String> = words.iter().map(|w| w.to_ascii_uppercase()).collect();
    let spelled = |s: &str| {
        let n = s.split(' ').count();
        (upper.len() >= n && upper[..n].join(" ") == s).then_some(n)
    };
    COMMANDS
        .iter()
        .flat_map(|c| c.aliases.iter().chain([&c.name]).filter_map(move |s| Some((c, spelled(s)?))))
        .max_by_key(|&(_, n)| n)
}

pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

const RELATIONS: &[&str] = &[">=", "<=", "¬=", "=>", "=<", "=", ">", "<", "EQ", "NE", "GT", "GE", "LT", "LE"];
const CONNECTORS: &[&str] = &["AND", "OR", "&", "|", "*", "+"];

/// A WHERE argument's comparisons, each a segment field, a relational operator and the value's
/// text: a data name or a literal; `Err` names what is wrong.
pub fn qualification(text: &str) -> Result<Vec<(String, String, String)>, String> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let field_end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '#' | '@' | '$'))).unwrap_or(rest.len());
        let field = &rest[..field_end];
        if field.is_empty() {
            return Err(format!("a segment field name, not {rest}"));
        }
        rest = rest[field_end..].trim_start();
        let relation = RELATIONS
            .iter()
            .find(|r| rest.get(..r.len()).is_some_and(|p| p.eq_ignore_ascii_case(r)) && (r.chars().all(|c| !c.is_ascii_alphabetic()) || rest[r.len()..].starts_with(' ')))
            .ok_or_else(|| format!("a relational operator after {field}"))?;
        rest = rest[relation.len()..].trim_start();
        let (value, after) = value(rest);
        if value.is_empty() {
            return Err(format!("a value after {field} {relation}"));
        }
        out.push((field.to_ascii_uppercase(), relation.to_ascii_uppercase(), value.to_owned()));
        rest = after.trim_start();
        if let Some(c) = CONNECTORS.iter().find(|c| rest.get(..c.len()).is_some_and(|p| p.eq_ignore_ascii_case(c)) && rest[c.len()..].starts_with([' ', '\t'])) {
            rest = rest[c.len()..].trim_start();
        }
    }
    Ok(out)
}

/// A value at the start of `text`, a quoted literal whole, and what follows it.
fn value(text: &str) -> (&str, &str) {
    if let Some(q @ ('\'' | '"')) = text.chars().next() {
        let mut end = 1;
        let bytes = text.as_bytes();
        while end < bytes.len() {
            if bytes[end] == q as u8 {
                if bytes.get(end + 1) == Some(&(q as u8)) {
                    end += 2;
                    continue;
                }
                return (&text[..=end], &text[end + 1..]);
            }
            end += 1;
        }
        return (text, "");
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..end], &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_spellings_name_their_command() {
        let words = |s: &'static str| s.split_whitespace().collect::<Vec<_>>();
        assert_eq!(command(&words("GET NEXT IN PARENT USING PCB(1)")).map(|(c, n)| (c.name, n)), Some(("GNP", 4)));
        assert_eq!(command(&words("gn using pcb(1)")).map(|(c, n)| (c.name, n)), Some(("GN", 1)));
        assert!(command(&words("FETCH X")).is_none());
    }

    #[test]
    fn a_qualification_is_comparisons_joined_by_and_or_or() {
        assert_eq!(qualification("ACCNTID = PA-ACCT-ID").unwrap(), [("ACCNTID".into(), "=".into(), "PA-ACCT-ID".into())]);
        let q = qualification("KEYA > SEGKEY1 AND KEYA < 'A 350' OR KEYA>=X").unwrap();
        assert_eq!(q.iter().map(|(f, r, v)| format!("{f}{r}{v}")).collect::<Vec<_>>(), ["KEYA>SEGKEY1", "KEYA<'A 350'", "KEYA>=X"]);
        assert!(qualification("= X").is_err());
        assert!(qualification("KEYA X").is_err());
    }
}
