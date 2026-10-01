//! DFSORT control statements from SYSIN: SORT and MERGE with FIELDS, SUM FIELDS=NONE, OPTION and
//! RECORD, as DFSORT Application Programming Guide's control-statement chapter writes them. A
//! statement or operand these do not model (INCLUDE, OMIT, INREC, OUTREC, OUTFIL, a SUM of fields,
//! exits) is refused by name.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ch,
    Ac,
    Zd,
    Clo,
    Csl,
    Cst,
    Pd,
    Bi,
    Fi,
}

/// One control field: `length` bytes from `position` (1 is the record's first byte, the RDW's
/// first for a variable-length record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub position: usize,
    pub length: usize,
    pub format: Format,
    pub ascending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sort,
    Merge,
    Copy,
}

/// RECORD TYPE=F|V and the first LENGTH value: the input's record format when its DD gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub variable: bool,
    pub length: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub kind: Kind,
    pub fields: Vec<Field>,
    /// SUM FIELDS=NONE: of records with equal keys, the first is kept.
    pub drop_duplicates: bool,
    pub record: Option<Record>,
}

const OPTION_IGNORED: &[&str] = &["EQUALS", "NOEQUALS", "DYNALLOC", "FILSZ", "SIZE", "MAINSIZE", "AVGRLEN", "MSGPRT", "LIST", "NOLIST", "LISTX", "NOLISTX", "STOPAFT"];
const REFUSED: &[&str] = &["INCLUDE", "OMIT", "INREC", "OUTREC", "OUTFIL", "MODS", "ALTSEQ", "JOINKEYS", "JOIN", "REFORMAT", "DEBUG", "ALTER"];

/// Each statement's text: columns 1-71 after a label-free column 1, comment cards (`*` in column
/// 1) dropped, and a card whose operands end in a comma continued on the next.
fn statements(cards: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut continuing = false;
    for card in cards {
        if card.starts_with('*') {
            continue;
        }
        let text: String = card.chars().take(71).collect();
        let body = text.trim();
        if body.is_empty() {
            continue;
        }
        // A statement is its verb and one operand field; a continued card holds only operands.
        let first = body.split_whitespace().take(if continuing { 1 } else { 2 }).collect::<Vec<_>>().join(" ");
        if continuing {
            if let Some(last) = out.last_mut() {
                last.push_str(&first);
            }
        } else {
            out.push(first);
        }
        continuing = out.last().is_some_and(|s| s.ends_with(','));
    }
    out
}

fn split_top(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn format_of(word: &str) -> Result<Format, String> {
    Ok(match word {
        "CH" => Format::Ch,
        "AC" => Format::Ac,
        "ZD" => Format::Zd,
        "CLO" | "OL" => Format::Clo,
        "CSL" | "LS" => Format::Csl,
        "CST" | "TS" => Format::Cst,
        "PD" => Format::Pd,
        "BI" => Format::Bi,
        "FI" => Format::Fi,
        f => return Err(format!("the field format {f} is not supported yet")),
    })
}

fn number(text: &str, what: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{text} is not a {what}")),
    }
}

/// FIELDS=(p,l,f,o,...), (p,l,o,...) with FORMAT=f, or COPY.
fn fields(value: &str, format: Option<Format>) -> Result<Option<Vec<Field>>, String> {
    if value == "COPY" {
        return Ok(None);
    }
    let inner = value.strip_prefix('(').and_then(|v| v.strip_suffix(')')).ok_or_else(|| format!("FIELDS={value} is not in parentheses"))?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let width = if format.is_some() { 3 } else { 4 };
    if parts.is_empty() || !parts.len().is_multiple_of(width) {
        return Err(format!("FIELDS=({inner}) is not position, length{}, order for each field", if format.is_some() { "" } else { ", format" }));
    }
    let mut out = Vec::new();
    for group in parts.chunks(width) {
        let format = match format {
            Some(f) => f,
            None => format_of(group[2])?,
        };
        let ascending = match group[width - 1] {
            "A" => true,
            "D" => false,
            "E" => return Err("an E order (an exit's own) is not supported yet".into()),
            o => return Err(format!("{o} is not A or D")),
        };
        out.push(Field { position: number(group[0], "position")?, length: number(group[1], "length")?, format, ascending });
    }
    Ok(Some(out))
}

/// The control statements in SYSIN's cards.
pub fn parse(cards: &[String]) -> Result<Control, String> {
    let mut control = Control { kind: Kind::Copy, fields: Vec::new(), drop_duplicates: false, record: None };
    let mut verb_seen = false;
    for statement in statements(cards) {
        let (verb, operands) = statement.split_once(' ').unwrap_or((statement.as_str(), ""));
        let ops = split_top(operands);
        let keyword = |name: &str| ops.iter().find_map(|o| o.strip_prefix(&format!("{name}=")).map(str::to_string));
        match verb {
            "SORT" | "MERGE" => {
                if verb_seen {
                    return Err("more than one SORT or MERGE statement".into());
                }
                verb_seen = true;
                let format = keyword("FORMAT").map(|f| format_of(&f)).transpose()?;
                let value = keyword("FIELDS").ok_or_else(|| format!("{verb} needs FIELDS="))?;
                if let Some(other) = ops.iter().find(|o| !o.starts_with("FIELDS=") && !o.starts_with("FORMAT=") && !matches!(o.as_str(), "EQUALS" | "NOEQUALS") && !o.starts_with("FILSZ=") && !o.starts_with("SIZE=")) {
                    return Err(format!("{verb} operand {other} is not supported yet"));
                }
                match fields(&value, format)? {
                    None => control.kind = Kind::Copy,
                    Some(f) => {
                        control.kind = if verb == "SORT" { Kind::Sort } else { Kind::Merge };
                        control.fields = f;
                    }
                }
            }
            "SUM" => match keyword("FIELDS").as_deref() {
                Some("NONE") | Some("(NONE)") => control.drop_duplicates = true,
                _ => return Err("SUM of fields is not supported yet; SUM FIELDS=NONE is".into()),
            },
            "OPTION" => {
                for o in &ops {
                    let name = o.split(['=', '(']).next().unwrap_or("");
                    match name {
                        "COPY" => {
                            verb_seen = true;
                            control.kind = Kind::Copy;
                        }
                        n if OPTION_IGNORED.contains(&n) => {}
                        n => return Err(format!("OPTION {n} is not supported yet")),
                    }
                }
            }
            "RECORD" => {
                let variable = match keyword("TYPE").as_deref() {
                    Some("F") | None => false,
                    Some("V" | "VB") => true,
                    Some(t) => return Err(format!("RECORD TYPE={t} is not supported yet")),
                };
                let length = keyword("LENGTH").and_then(|l| l.trim_start_matches('(').split([',', ')']).next().map(str::to_string)).filter(|l| !l.is_empty()).map(|l| number(&l, "record length")).transpose()?;
                control.record = Some(Record { variable, length });
            }
            "END" => break,
            v if REFUSED.contains(&v) => return Err(format!("the DFSORT {v} statement is not supported yet")),
            v => return Err(format!("{v} is not a DFSORT control statement")),
        }
    }
    if !verb_seen {
        return Err("no SORT, MERGE or OPTION COPY statement".into());
    }
    Ok(control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(text: &str) -> Vec<String> {
        text.lines().map(|l| if l.starts_with('*') { l.to_string() } else { format!(" {l}") }).collect()
    }

    #[test]
    fn sort_fields_with_and_without_format() {
        let c = parse(&cards("  SORT FIELDS=(1,8,CH,A,10,5,PD,D)\n  SUM FIELDS=NONE")).unwrap();
        assert_eq!(c.kind, Kind::Sort);
        assert_eq!(c.fields, [Field { position: 1, length: 8, format: Format::Ch, ascending: true }, Field { position: 10, length: 5, format: Format::Pd, ascending: false }]);
        assert!(c.drop_duplicates);
        let c = parse(&cards("  SORT FIELDS=(1,4,A,9,2,D),FORMAT=ZD,EQUALS")).unwrap();
        assert_eq!(c.fields.iter().map(|f| (f.format, f.ascending)).collect::<Vec<_>>(), [(Format::Zd, true), (Format::Zd, false)]);
        assert_eq!(parse(&cards("  MERGE FIELDS=(5,3,BI,A)")).unwrap().kind, Kind::Merge);
        assert_eq!(parse(&cards("  SORT FIELDS=COPY")).unwrap().kind, Kind::Copy);
        assert_eq!(parse(&cards("  OPTION COPY")).unwrap().kind, Kind::Copy);
    }

    #[test]
    fn a_continued_statement_and_comments() {
        let c = parse(&cards("* the keys\n  SORT FIELDS=(1,8,CH,A,\n               20,4,FI,D)    THE REST IS A COMMENT\n  RECORD TYPE=V,LENGTH=(84)")).unwrap();
        assert_eq!(c.fields.len(), 2);
        assert_eq!(c.record, Some(Record { variable: true, length: Some(84) }));
    }

    #[test]
    fn what_is_not_modelled_is_refused_by_name() {
        for (text, message) in [
            ("  SORT FIELDS=(1,8,CH,A)\n  INCLUDE COND=(1,1,CH,EQ,C'A')", "the DFSORT INCLUDE statement is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTFIL FNAMES=OUT1", "the DFSORT OUTFIL statement is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  SUM FIELDS=(10,5,PD)", "SUM of fields is not supported yet; SUM FIELDS=NONE is"),
            ("  SORT FIELDS=(1,8,FL,A)", "the field format FL is not supported yet"),
            ("  SORT FIELDS=(1,8,CH)", "FIELDS=(1,8,CH) is not position, length, format, order for each field"),
            ("  OPTION VLSHRT\n  SORT FIELDS=COPY", "OPTION VLSHRT is not supported yet"),
            ("  SUM FIELDS=NONE", "no SORT, MERGE or OPTION COPY statement"),
        ] {
            assert_eq!(parse(&cards(text)).unwrap_err(), message, "{text}");
        }
    }
}
