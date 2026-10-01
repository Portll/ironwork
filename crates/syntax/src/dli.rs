//! EXEC DLI commands as the IMS translator reads them (IMS Application Programming: EXEC DLI
//! Commands for CICS and IMS, SC18-7811-04, chapters 4-6): each command, the longer spellings it
//! has, and the options it takes.

pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub options: &'static [&'static str],
}

const GET: &[&str] = &[
    "PCB", "AIB", "KEYFEEDBACK", "FEEDBACKLEN", "INTO", "VARIABLE", "OFFSET", "SEGLENGTH", "LOCKED", "LOCKCLASS", "MOVENEXT", "GETFIRST", "SET", "SETCOND",
    "SETZERO", "SETPARENT", "FIRST", "LAST", "CURRENT", "SEGMENT", "FIELDLENGTH", "KEYLENGTH", "KEYS", "WHERE",
];

pub const COMMANDS: &[Command] = &[
    Command { name: "GU", aliases: &["GET UNIQUE"], options: GET },
    Command { name: "GHU", aliases: &["GET HOLD UNIQUE"], options: GET },
    Command { name: "GN", aliases: &["GET NEXT"], options: GET },
    Command { name: "GHN", aliases: &["GET HOLD NEXT"], options: GET },
    Command { name: "GNP", aliases: &["GET NEXT IN PARENT"], options: GET },
    Command { name: "GHNP", aliases: &["GET HOLD NEXT IN PARENT"], options: GET },
    Command { name: "ISRT", aliases: &["INSERT"], options: &["PCB", "AIB", "VARIABLE", "SEGMENT", "SEGLENGTH", "FROM", "OFFSET", "KEYS", "KEYLENGTH", "WHERE", "FIELDLENGTH", "FIRST", "LAST", "CURRENT", "MOVENEXT", "GETFIRST", "SET", "SETCOND", "SETZERO", "SETPARENT"] },
    Command { name: "DLET", aliases: &["DELETE"], options: &["PCB", "AIB", "VARIABLE", "SEGMENT", "SEGLENGTH", "FROM", "SETZERO"] },
    Command { name: "REPL", aliases: &["REPLACE"], options: &["PCB", "AIB", "VARIABLE", "SEGMENT", "SEGLENGTH", "FROM", "OFFSET", "MOVENEXT", "SET", "SETCOND", "SETZERO"] },
    Command { name: "SCHD", aliases: &["SCHEDULE"], options: &["PSB", "SYSSERVE", "NODHABEND"] },
    Command { name: "TERM", aliases: &["TERMINATE"], options: &[] },
    Command { name: "CHKP", aliases: &["CHECKPOINT"], options: &["ID", "AIB"] },
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
