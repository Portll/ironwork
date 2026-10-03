//! DFSORT symbols, as the Application Programming Guide's SYMNAMES chapter writes them: symbol
//! statements naming a field (p,m,f, p,m or p, with * and = for the next and previous values) or a
//! constant, the POSITION, SKIP and ALIGN keyword statements, comment and blank statements; and
//! the substitution of a symbol where a control statement takes a field, a constant or a column.

use std::collections::HashMap;

/// What a symbol stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Symbol {
    /// p,m,f or p,m: a field. Its format is kept as written, upper-cased.
    Field { position: usize, length: usize, format: Option<String> },
    /// p alone, or n: a position or a decimal constant, by where it is used.
    Number(String),
    /// A character, hexadecimal or signed decimal constant, as a control statement writes it.
    Constant(String),
    /// A constant or parsed field these statements do not model, refused where it is used.
    Unsupported(String),
}

#[derive(Debug, Clone, Default)]
pub struct Symbols {
    map: HashMap<String, Symbol>,
    order: Vec<String>,
}

/// The formats a symbol statement may give a field.
const FORMATS: &[&str] = &[
    "AC", "AQ", "ASL", "AST", "BI", "CH", "CLO", "CSF", "CSL", "CST", "CTO", "DC1", "DC2", "DC3", "DE1", "DE2", "DE3", "DT1", "DT2", "DT3", "D1", "D2", "FI", "FL", "FS", "LS", "OL", "OT", "PD", "PD0", "SFF", "SS", "TC1", "TC2", "TC3", "TC4", "TE1", "TE2",
    "TE3", "TE4", "TM1", "TM2", "TM3", "TM4", "TS", "UFF", "Y2B", "Y2C", "Y2D", "Y2DP", "Y2P", "Y2PP", "Y2S", "Y2T", "Y2TP", "Y2U", "Y2UP", "Y2V", "Y2VP", "Y2W", "Y2WP", "Y2X", "Y2XP", "Y2Y", "Y2YP", "Y2Z", "Y4T", "Y4U", "Y4V", "Y4W", "Y4X", "Y4Y", "ZD",
];

/// DFSORT's reserved words, which are not symbols in upper case.
const RESERVED: &[&str] = &[
    "A", "AC", "ADD", "ADDDAYS", "ADDMONS", "ADDYEARS", "ALL", "AND", "AQ", "ASF", "ASL", "AST", "AUF", "BI", "CH", "CLO", "COPY", "COUNT", "COUNT15", "CSF", "CSL", "CST", "CTO", "D", "DATE", "DATEDIFF", "DATE1", "DATE2", "DATE3", "DATE4", "DATE5", "DC1", "DC2",
    "DC3", "DC4", "DE1", "DE2", "DE3", "DE4", "DIV", "DT1", "DT2", "DT3", "D1", "D2", "E", "F", "FI", "FL", "FS", "H", "HEX", "LASTDAYM", "LASTDAYQ", "LASTDAYW", "LASTDAYY", "LC", "LN", "LS", "MAX", "MC", "MIN", "MN", "MOD", "MUL", "NONE", "NUM", "OL", "OR", "OT",
    "PAGE", "PAGEHEAD", "PD", "PDC", "PDF", "PD0", "SEQNUM", "SFF", "SS", "SUB", "SUBCOUNT", "SUBCOUNT15", "SUBDAYS", "SUBMONS", "SUBYEARS", "TC1", "TC2", "TC3", "TC4", "TE1", "TE2", "TE3", "TE4", "TIME", "TIME1", "TIME1P", "TIME2", "TIME2P", "TIME3", "TIME3P",
    "TM1", "TM2", "TM3", "TM4", "TS", "UC", "UFF", "UN", "UTF8", "UTF16", "UTF32", "VALCNT", "VLEN", "X", "Z", "ZD", "ZDC", "ZDF",
];

fn reserved(name: &str) -> bool {
    let b = name.as_bytes();
    let digits = |s: &[u8]| !s.is_empty() && s.iter().all(u8::is_ascii_digit);
    RESERVED.contains(&name)
        || (b.first() == Some(&b'M') && b.len() <= 3 && digits(&b[1..]))
        || name.starts_with("DATE1") || name.starts_with("DATE2") || name.starts_with("DATE3")
        || ((name.starts_with("Y2") || name.starts_with("Y4")) && (3..=4).contains(&b.len()))
        || ["NEXTD", "PREVD"].iter().any(|p| name.strip_prefix(p).is_some_and(|d| ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"].contains(&d)))
}

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty() && b.len() <= 50 && !b[0].is_ascii_digit() && b[0] != b'-' && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'#' | b'$' | b'@' | b'_' | b'-')) && !reserved(name)
}

/// The text up to the first blank outside apostrophes: a statement's value, before its remark.
fn value_field(text: &str) -> &str {
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ' ' if !quoted => return &text[..i],
            _ => {}
        }
    }
    text
}

fn number(text: &str, what: &str, line: usize) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(n) if (1..=32752).contains(&n) => Ok(n),
        _ => Err(format!("SYMNAMES line {line}: {text} is not a {what} from 1 to 32752")),
    }
}

/// Where the next * and = take their values from.
#[derive(Default)]
struct Cursor {
    next: Option<usize>,
    position: Option<usize>,
    length: Option<usize>,
    format: Option<String>,
}

impl Symbols {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.map.get(name)
    }

    /// The symbols in the SYMNAMES data set's lines, in order.
    pub fn read(lines: &[String]) -> Result<Symbols, String> {
        let mut out = Symbols::default();
        let mut cursor = Cursor::default();
        for (i, raw) in lines.iter().enumerate() {
            let line = i + 1;
            let text: String = raw.chars().take(80).collect();
            if text.starts_with('*') || text.trim().is_empty() {
                continue;
            }
            let statement = value_field(text.trim_start());
            let Some((name, value)) = statement.split_once([',', ';']) else { return Err(format!("SYMNAMES line {line}: {statement} is not symbol,value")) };
            match name {
                "POSITION" => {
                    let at = match out.map.get(value) {
                        Some(Symbol::Field { position, .. }) => *position,
                        _ => number(value, "position", line)?,
                    };
                    cursor.next = Some(at);
                    cursor.position = Some(at);
                }
                "SKIP" => cursor.next = Some(cursor.next.unwrap_or(1) + number(value, "count", line)?),
                "ALIGN" => {
                    let boundary = match value.to_ascii_uppercase().as_str() {
                        "H" => 2,
                        "F" => 4,
                        "D" => 8,
                        v => return Err(format!("SYMNAMES line {line}: ALIGN takes H, F or D, not {v}")),
                    };
                    let next = cursor.next.unwrap_or(1);
                    cursor.next = Some(next + (boundary - (next - 1) % boundary) % boundary);
                }
                _ => {
                    if !valid_name(name) {
                        return Err(format!("SYMNAMES line {line}: {name} is not a symbol, or is a reserved word"));
                    }
                    if out.map.contains_key(name) {
                        return Err(format!("SYMNAMES line {line}: {name} is defined twice"));
                    }
                    let symbol = Self::value(value, &mut cursor, line)?;
                    out.order.push(name.to_string());
                    out.map.insert(name.to_string(), symbol);
                }
            }
        }
        Ok(out)
    }

    fn value(value: &str, cursor: &mut Cursor, line: usize) -> Result<Symbol, String> {
        let upper = value.to_ascii_uppercase();
        if value.starts_with('\'') || upper.starts_with("C'") || upper.starts_with("X'") {
            let body = if value.starts_with('\'') { value } else { &value[1..] };
            if !body.ends_with('\'') || body.len() < 2 {
                return Err(format!("SYMNAMES line {line}: {value} is not a closed string"));
            }
            let kind = if upper.starts_with("X'") { 'X' } else { 'C' };
            return Ok(Symbol::Constant(format!("{kind}{body}")));
        }
        if ["B'", "S'", "Y'"].iter().any(|p| upper.starts_with(p)) {
            return Ok(Symbol::Unsupported(format!("the {} string {value}", &upper[..1])));
        }
        if value.starts_with('%') {
            return Ok(Symbol::Unsupported(format!("the parsed field {value}")));
        }
        if value.starts_with(['+', '-']) {
            let digits = &value[1..];
            if digits.is_empty() || digits.len() > 31 || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("SYMNAMES line {line}: {value} is not a decimal number"));
            }
            return Ok(Symbol::Constant(value.to_string()));
        }
        let parts: Vec<&str> = value.split(',').collect();
        if parts.len() == 1 && parts[0].bytes().all(|b| b.is_ascii_digit()) && !parts[0].is_empty() {
            return Ok(Symbol::Number(parts[0].to_string()));
        }
        let position = match parts[0] {
            "*" => cursor.next.unwrap_or(1),
            "=" => cursor.position.ok_or_else(|| format!("SYMNAMES line {line}: = for a position before any position was set"))?,
            p => number(p, "position", line)?,
        };
        if parts.len() == 1 {
            return Ok(Symbol::Number(position.to_string()));
        }
        let length = match parts[1] {
            "=" => cursor.length.ok_or_else(|| format!("SYMNAMES line {line}: = for a length before any length was set"))?,
            m => number(m, "length", line)?,
        };
        let format = match parts.get(2).map(|f| f.to_ascii_uppercase()) {
            None => None,
            Some(f) if f == "=" => Some(cursor.format.clone().ok_or_else(|| format!("SYMNAMES line {line}: = for a format before any format was set"))?),
            Some(f) if FORMATS.contains(&f.as_str()) => Some(f),
            Some(f) => return Err(format!("SYMNAMES line {line}: {f} is not a field format")),
        };
        if parts.len() > 3 {
            return Err(format!("SYMNAMES line {line}: {value} is not p,m,f"));
        }
        cursor.next = Some(position + length);
        cursor.position = Some(position);
        cursor.length = Some(length);
        if format.is_some() {
            cursor.format.clone_from(&format);
        }
        Ok(Symbol::Field { position, length, format })
    }

    /// The symbol table as SYMNOUT shows it: one symbol a line, with the positions * and = took.
    pub fn table(&self) -> Vec<String> {
        self.order
            .iter()
            .map(|name| match &self.map[name] {
                Symbol::Field { position, length, format: Some(f) } => format!("{name},{position},{length},{f}"),
                Symbol::Field { position, length, format: None } => format!("{name},{position},{length}"),
                Symbol::Number(n) => format!("{name},{n}"),
                Symbol::Constant(c) => format!("{name},{c}"),
                Symbol::Unsupported(what) => format!("{name}: {what}"),
            })
            .collect()
    }

    fn unsupported<T>(what: &str, name: &str) -> Result<T, String> {
        Err(format!("the symbol {name} stands for {what}, which these statements do not model yet"))
    }

    /// `token` as the field or constant a comparison takes: p,m,f (p,m under FORMAT=), or the
    /// constant. None when it is no symbol.
    pub fn in_condition(&self, token: &str, format_given: bool) -> Result<Option<Vec<String>>, String> {
        Ok(Some(match self.map.get(token) {
            None => return Ok(None),
            Some(Symbol::Field { position, length, format }) => {
                let mut out = vec![position.to_string(), length.to_string()];
                if let (false, Some(f)) = (format_given, format) {
                    out.push(f.clone());
                }
                out
            }
            Some(Symbol::Number(n)) => vec![n.clone()],
            Some(Symbol::Constant(c)) => vec![c.clone()],
            Some(Symbol::Unsupported(what)) => return Self::unsupported(what, token),
        }))
    }

    /// `token` as a SORT or MERGE field: p,m,f, or p,m under FORMAT=.
    pub fn in_sort_fields(&self, token: &str, format_given: bool) -> Result<Option<Vec<String>>, String> {
        match self.map.get(token) {
            None => Ok(None),
            Some(Symbol::Field { position, length, format }) => {
                let mut out = vec![position.to_string(), length.to_string()];
                match (format_given, format) {
                    (true, _) => {}
                    (false, Some(f)) => out.push(f.clone()),
                    (false, None) => return Err(format!("the symbol {token} has no format, and no FORMAT= gives one")),
                }
                Ok(Some(out))
            }
            Some(Symbol::Unsupported(what)) => Self::unsupported(what, token),
            Some(_) => Err(format!("the symbol {token} is a constant, not a field to sort on")),
        }
    }

    /// The column `name:` names: a field's position, or a number.
    pub fn column(&self, name: &str) -> Option<usize> {
        match self.map.get(name) {
            Some(Symbol::Field { position, .. }) => Some(*position),
            Some(Symbol::Number(n)) => n.parse().ok(),
            _ => None,
        }
    }

    /// `token` as a reformatting item: p,m (p,m,f when `edited`, an edit or conversion following),
    /// p for a position, or the constant.
    pub fn in_items(&self, token: &str, edited: bool) -> Result<Option<Vec<String>>, String> {
        Ok(Some(match self.map.get(token) {
            None => return Ok(None),
            Some(Symbol::Field { position, length, format }) => {
                let mut out = vec![position.to_string(), length.to_string()];
                if let (true, Some(f)) = (edited, format) {
                    out.push(f.clone());
                }
                out
            }
            Some(Symbol::Number(n)) => vec![n.clone()],
            Some(Symbol::Constant(c)) => vec![c.clone()],
            Some(Symbol::Unsupported(what)) => return Self::unsupported(what, token),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Symbols, String> {
        Symbols::read(&text.lines().map(str::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn star_and_equals_take_the_next_and_previous_values() {
        let s = read("Sym1,*,5,ZD\nCon1,27\nSym2,*,2,BI\nField1,8,13,CH\nField2,*,5,PD\nField3,*,2,FI").unwrap();
        assert_eq!(s.table(), ["Sym1,1,5,ZD", "Con1,27", "Sym2,6,2,BI", "Field1,8,13,CH", "Field2,21,5,PD", "Field3,26,2,FI"]);
        let s = read("Sym1,5,4,CH\nSym2,=,2,CH\nSym3,*,2,CH").unwrap();
        assert_eq!(s.table(), ["Sym1,5,4,CH", "Sym2,5,2,CH", "Sym3,7,2,CH"]);
        let s = read("Field1,5,8,CH\nField1a,=,3\nField2,*,12,=\nField3,*,20,=").unwrap();
        assert_eq!(s.table(), ["Field1,5,8,CH", "Field1a,5,3", "Field2,8,12,CH", "Field3,20,20,CH"]);
    }

    #[test]
    fn keyword_statements_move_the_next_position() {
        let s = read("POSITION,27\nAccount_Balance,*,5,PD\nAccount_Id,*,8,CH\nPOSITION,84\nNew_Balance,=,20").unwrap();
        assert_eq!(s.table(), ["Account_Balance,27,5,PD", "Account_Id,32,8,CH", "New_Balance,84,20"]);
        let s = read("Field#1,15,6,FS\n SKIP,4 Unused bytes\nField#2,*,5,=\n SKIP,2\nField#3,*,8,CH").unwrap();
        assert_eq!(s.table(), ["Field#1,15,6,FS", "Field#2,25,5,FS", "Field#3,32,8,CH"]);
        let s = read("A1,7,3,CH\nALIGN,H\nA2,*,2,BI\nB1,7,3,CH\nALIGN,f\nB2,*,4,BI\nC1,7,3,CH\nALIGN,D\nC2,*,8,BI").unwrap();
        assert_eq!(s.table(), ["A1,7,3,CH", "A2,11,2,BI", "B1,7,3,CH", "B2,13,4,BI", "C1,7,3,CH", "C2,17,8,BI"]);
        let s = read("Workarea,21,100\n volser1,=,6,CH\n volser2,*,6,CH\nPOSITION,Workarea\n status,=,1,BI\n dsname,*,44,CH").unwrap();
        assert_eq!(s.table(), ["Workarea,21,100", "volser1,21,6,CH", "volser2,27,6,CH", "status,21,1,BI", "dsname,22,44,CH"]);
    }

    #[test]
    fn constants_remarks_and_comments() {
        let s = read("* a comment\n\nMy_Title,c'My Report' a remark\nDEPT1;'J82'\nStopper,X'FFFFFF'\nLIMIT,+12500\nFlags,B'11010000'").unwrap();
        assert_eq!(s.table(), ["My_Title,C'My Report'", "DEPT1,C'J82'", "Stopper,X'FFFFFF'", "LIMIT,+12500", "Flags: the B string B'11010000'"]);
        assert!(s.in_condition("Flags", false).unwrap_err().contains("do not model yet"));
        assert!(read("COUNT,1,2,CH").unwrap_err().contains("reserved word"));
        assert!(read("M12,1,2,CH").unwrap_err().contains("reserved word"));
        assert!(read("count,1,2,CH").is_ok(), "only the upper-case form is reserved");
        assert!(read("1st,1,2,CH").unwrap_err().contains("not a symbol"));
        assert!(read("X1,=,2").unwrap_err().contains("before any position"));
    }

    #[test]
    fn a_symbol_is_substituted_as_its_place_takes_it() {
        let s = read("C_Field1,6,5,CH\nAny_Format,12,3\nMax,200000\nCode,c'86A4Z'").unwrap();
        assert_eq!(s.in_condition("C_Field1", false).unwrap(), Some(vec!["6".into(), "5".into(), "CH".into()]));
        assert_eq!(s.in_condition("C_Field1", true).unwrap(), Some(vec!["6".into(), "5".into()]));
        assert_eq!(s.in_condition("Max", false).unwrap(), Some(vec!["200000".into()]));
        assert_eq!(s.in_condition("Code", false).unwrap(), Some(vec!["C'86A4Z'".into()]));
        assert_eq!(s.in_sort_fields("C_Field1", true).unwrap(), Some(vec!["6".into(), "5".into()]));
        assert!(s.in_sort_fields("Any_Format", false).unwrap_err().contains("no format"));
        assert_eq!(s.in_items("C_Field1", false).unwrap(), Some(vec!["6".into(), "5".into()]));
        assert_eq!(s.in_items("C_Field1", true).unwrap(), Some(vec!["6".into(), "5".into(), "CH".into()]));
        assert_eq!(s.column("Max"), Some(200000));
        assert_eq!(s.in_condition("NOT_A_SYMBOL", false).unwrap(), None);
    }
}
