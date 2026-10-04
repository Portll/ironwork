//! BMS map source (the DFHMSD, DFHMDI and DFHMDF assembler macros) as a mapset model, and the
//! COBOL symbolic map a program COPYs for it.

use crate::copy::Libraries;
use crate::messages::Message;
use crate::{Error, Pos};
use std::fmt::Write as _;
use std::path::PathBuf;

pub use rt::bms::{Attrb, Field, Initial, Intensity, Map, Mapset, Mode, Protection, Slot, extended_attributes, picture_size, slots};
use rt::bms::EXTENDED;

/// The mapset a COPY of `name` finds in the libraries: the one named `name` in `NAME.bms`, or the
/// file's only mapset. None when no library holds a BMS file of that name.
pub fn find_mapset(libraries: &Libraries, name: &str) -> Option<Result<Mapset, Error>> {
    load(libraries, name, None).map(|(_, mapset)| mapset)
}

pub(crate) fn load(libraries: &Libraries, name: &str, library: Option<&str>) -> Option<(PathBuf, Result<Mapset, Error>)> {
    let path = libraries.find_bms(name, library)?;
    let mapset = std::fs::read(&path)
        .map_err(|e| crate::messages::IWP0045.at(Pos::default(), e.to_string()))
        .map(|bytes| crate::copy::decode(&bytes))
        .and_then(|text| parse(&text))
        .and_then(|mut sets| {
            match sets.iter().position(|s| s.name.eq_ignore_ascii_case(name)) {
                Some(i) => Ok(sets.swap_remove(i)),
                None if sets.len() == 1 => Ok(sets.remove(0)),
                None => Err(crate::messages::IWP0001.at(Pos::default(), format!("no mapset {} among the {} in the file", name.to_ascii_uppercase(), sets.len()))),
            }
        })
        .map_err(|mut e| {
            e.file = Some(path.display().to_string());
            e
        });
    Some((path, mapset))
}

#[derive(Clone, Debug)]
enum Val {
    Word(String),
    Quoted(String),
    List(Vec<String>),
}

struct Statement {
    line: u32,
    label: Option<String>,
    macro_name: String,
    operands: Vec<(String, Val)>,
}

fn fail(line: u32, message: Message, text: impl Into<String>) -> Error {
    message.at(Pos { file: 0, line, col: 1 }, text)
}

/// The statements of the source: comments dropped, continuation lines joined, columns 73-80 ignored.
fn statements(text: &str) -> Result<Vec<Statement>, Error> {
    let mut out = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, raw)) = lines.next() {
        let start = n as u32 + 1;
        let line: Vec<char> = raw.chars().take(72).collect();
        if line.first() == Some(&'*') || line.starts_with(&['.', '*']) || line.iter().all(|c| c.is_whitespace()) {
            continue;
        }
        let mut joined = String::new();
        let mut from = 0;
        let mut line = line;
        loop {
            let continued = line.len() == 72 && line[71] != ' ';
            let end = if continued { 71 } else { line.len() };
            joined.extend(line.get(from..end).unwrap_or_default());
            if !continued {
                break;
            }
            let (_, next) = lines.next().ok_or_else(|| fail(start, crate::messages::IWP0009, "a continuation line is missing"))?;
            line = next.chars().take(72).collect();
            from = 15;
        }
        out.push(statement(&joined, start)?);
    }
    Ok(out)
}

fn statement(text: &str, line: u32) -> Result<Statement, Error> {
    let token = |s: &str| s.split(char::is_whitespace).next().unwrap_or_default().len();
    let (label, rest) = if text.starts_with(char::is_whitespace) {
        (None, text.trim_start())
    } else {
        let n = token(text);
        (Some(text[..n].to_ascii_uppercase()), text[n..].trim_start())
    };
    let n = token(rest);
    let macro_name = rest[..n].to_ascii_uppercase();
    let rest = rest[n..].trim_start();
    if macro_name.is_empty() {
        return Err(fail(line, crate::messages::IWP0010, "a label with no macro"));
    }
    Ok(Statement { line, label, macro_name, operands: operands(rest, line)? })
}

/// `KEY=value` operands separated by commas; the first blank outside quotes and not after a comma
/// starts the remarks.
fn operands(text: &str, line: u32) -> Result<Vec<(String, Val)>, Error> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let key_start = i;
        while i < chars.len() && !matches!(chars[i], '=' | ',') && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect::<String>().to_ascii_uppercase();
        let mut value = Val::Word(String::new());
        if chars.get(i) == Some(&'=') {
            i += 1;
            value = match chars.get(i) {
                Some('\'') => {
                    let (s, next) = quoted(&chars, i, line)?;
                    i = next;
                    Val::Quoted(s)
                }
                Some('(') => {
                    let (items, next) = list(&chars, i, line)?;
                    i = next;
                    Val::List(items)
                }
                _ => {
                    let start = i;
                    let mut in_quote = false;
                    while i < chars.len() && (in_quote || !(chars[i] == ',' || chars[i].is_whitespace())) {
                        in_quote ^= chars[i] == '\'';
                        i += 1;
                    }
                    Val::Word(chars[start..i].iter().collect())
                }
            };
        }
        if !key.is_empty() {
            out.push((key, value));
        }
        if chars.get(i) == Some(&',') {
            i += 1;
            while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                i += 1;
            }
        } else {
            break;
        }
    }
    Ok(out)
}

/// The quoted string at `at` (`''` is one quote) and the index after its closing quote.
fn quoted(chars: &[char], at: usize, line: u32) -> Result<(String, usize), Error> {
    let mut s = String::new();
    let mut i = at + 1;
    while i < chars.len() {
        if chars[i] == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                s.push('\'');
                i += 2;
                continue;
            }
            return Ok((s, i + 1));
        }
        s.push(chars[i]);
        i += 1;
    }
    Err(fail(line, crate::messages::IWP0011, "a quoted string is not closed"))
}

fn list(chars: &[char], at: usize, line: u32) -> Result<(Vec<String>, usize), Error> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut i = at + 1;
    while i < chars.len() {
        match chars[i] {
            '\'' => {
                let (s, next) = quoted(chars, i, line)?;
                current.push_str(&s);
                i = next;
                continue;
            }
            ')' => {
                items.push(current.trim().to_owned());
                return Ok((items.into_iter().filter(|s| !s.is_empty()).collect(), i + 1));
            }
            ',' => items.push(std::mem::take(&mut current).trim().to_owned()),
            c => current.push(c),
        }
        i += 1;
    }
    Err(fail(line, crate::messages::IWP0012, "a parenthesised list is not closed"))
}

struct Operands<'a> {
    ops: &'a [(String, Val)],
    line: u32,
}

impl Operands<'_> {
    fn get(&self, key: &str) -> Option<&Val> {
        self.ops.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn words(&self, key: &str) -> Option<Vec<String>> {
        self.get(key).map(|v| match v {
            Val::List(items) => items.iter().map(|s| s.to_ascii_uppercase()).collect(),
            Val::Word(w) | Val::Quoted(w) => vec![w.to_ascii_uppercase()],
        })
    }

    fn word(&self, key: &str) -> Result<Option<String>, Error> {
        match self.words(key) {
            None => Ok(None),
            Some(mut w) if w.len() == 1 => Ok(w.pop()),
            Some(_) => Err(fail(self.line, crate::messages::IWP0013, format!("{key} takes one value"))),
        }
    }

    fn text(&self, key: &str) -> Result<Option<String>, Error> {
        match self.get(key) {
            None => Ok(None),
            Some(Val::Word(s) | Val::Quoted(s)) => Ok(Some(s.clone())),
            Some(Val::List(_)) => Err(fail(self.line, crate::messages::IWP0014, format!("{key} takes a quoted string"))),
        }
    }

    fn number(&self, key: &str, low: u32, high: u32) -> Result<Option<u32>, Error> {
        let Some(w) = self.word(key)? else { return Ok(None) };
        match w.parse::<u32>() {
            Ok(n) if (low..=high).contains(&n) => Ok(Some(n)),
            _ => Err(fail(self.line, crate::messages::IWP0015, format!("{key}={w} is not a number from {low} to {high}"))),
        }
    }

    fn yes_no(&self, key: &str) -> Result<Option<bool>, Error> {
        match self.word(key)?.as_deref() {
            None => Ok(None),
            Some("YES") => Ok(Some(true)),
            Some("NO") => Ok(Some(false)),
            Some(other) => Err(fail(self.line, crate::messages::IWP0016, format!("{key}={other}: YES or NO"))),
        }
    }

    /// DSATTS, or what EXTATT implies; None when neither is written.
    fn dsatts(&self) -> Result<Option<Vec<String>>, Error> {
        if let Some(list) = self.words("DSATTS") {
            let mut out: Vec<String> = Vec::new();
            for a in list {
                if !EXTENDED.iter().any(|(name, _, _)| *name == a) {
                    return Err(fail(self.line, crate::messages::IWP0017, format!("DSATTS={a} is not an extended attribute")));
                }
                if !out.contains(&a) {
                    out.push(a);
                }
            }
            return Ok(Some(out));
        }
        Ok(match self.word("EXTATT")?.as_deref() {
            None => None,
            Some("YES") => Some(EXTENDED.iter().filter(|(_, _, implied)| *implied).map(|(a, _, _)| a.to_string()).collect()),
            Some("NO" | "MAPONLY") => Some(Vec::new()),
            Some(other) => return Err(fail(self.line, crate::messages::IWP0018, format!("EXTATT={other}: NO, MAPONLY or YES"))),
        })
    }
}

const MAX_NAME: usize = 30;

struct OpenSet {
    set: Mapset,
    tioapfx: bool,
    dsatts: Vec<String>,
}

/// Every mapset in the source, each ended by `DFHMSD TYPE=FINAL` (or by the next mapset or the end).
pub fn parse(text: &str) -> Result<Vec<Mapset>, Error> {
    let mut done = Vec::new();
    let mut open: Option<OpenSet> = None;
    for st in statements(text)? {
        let ops = Operands { ops: &st.operands, line: st.line };
        match st.macro_name.as_str() {
            "PRINT" | "TITLE" | "EJECT" | "SPACE" | "END" => {}
            "DFHMSD" => {
                if ops.word("TYPE")?.as_deref() == Some("FINAL") {
                    done.push(open.take().ok_or_else(|| fail(st.line, crate::messages::IWP0019, "DFHMSD TYPE=FINAL with no mapset open"))?.set);
                    continue;
                }
                done.extend(open.take().map(|o| o.set));
                open = Some(mapset_header(&st, &ops)?);
            }
            "DFHMDI" => {
                let o = open.as_mut().ok_or_else(|| fail(st.line, crate::messages::IWP0020, "DFHMDI outside a DFHMSD"))?;
                let map = map_header(&st, &ops, o)?;
                o.set.maps.push(map);
            }
            "DFHMDF" => {
                let map = open.as_mut().and_then(|o| o.set.maps.last_mut()).ok_or_else(|| fail(st.line, crate::messages::IWP0021, "DFHMDF outside a DFHMDI map"))?;
                let field = field(&st, &ops, map)?;
                map.fields.push(field);
            }
            other => return Err(fail(st.line, crate::messages::IWP0022, format!("unknown macro {other}"))),
        }
    }
    done.extend(open.map(|o| o.set));
    Ok(done)
}

fn name_of(st: &Statement, what: &str, max: usize) -> Result<String, Error> {
    let name = st.label.clone().ok_or_else(|| fail(st.line, crate::messages::IWP0023, format!("{what} needs a name")))?;
    if name.len() > max {
        return Err(fail(st.line, crate::messages::IWP0024, format!("{what} name {name} is longer than {max} characters")));
    }
    Ok(name)
}

fn mapset_header(st: &Statement, ops: &Operands) -> Result<OpenSet, Error> {
    let name = name_of(st, "DFHMSD", MAX_NAME)?;
    // CardDemo's maps write &&SYSPARM, and CICS generated their copybooks from them.
    if let Some(t) = ops.word("TYPE")?
        && !matches!(t.as_str(), "DSECT" | "MAP" | "&SYSPARM" | "&&SYSPARM")
    {
        return Err(fail(st.line, crate::messages::IWP0025, format!("TYPE={t}: DSECT, MAP, FINAL or &SYSPARM")));
    }
    let mode = match ops.word("MODE")?.as_deref() {
        None | Some("OUT") => Mode::Out,
        Some("IN") => Mode::In,
        Some("INOUT") => Mode::InOut,
        Some(other) => return Err(fail(st.line, crate::messages::IWP0026, format!("MODE={other}: IN, OUT or INOUT"))),
    };
    let auto = ops.word("STORAGE")?.as_deref() == Some("AUTO");
    let tioapfx = ops.yes_no("TIOAPFX")?.unwrap_or(auto);
    Ok(OpenSet {
        set: Mapset { name, mode, ctrl: ops.words("CTRL").unwrap_or_default(), maps: Vec::new() },
        tioapfx,
        dsatts: ops.dsatts()?.unwrap_or_default(),
    })
}

fn map_header(st: &Statement, ops: &Operands, open: &OpenSet) -> Result<Map, Error> {
    let name = name_of(st, "DFHMDI", 7)?;
    let (lines, columns) = match ops.words("SIZE") {
        None => (24, 80),
        Some(v) => {
            let dims: Vec<u16> = v.iter().map(|s| s.parse().ok().filter(|n| (1..=240).contains(n)).ok_or(())).collect::<Result<_, _>>().map_err(|()| fail(st.line, crate::messages::IWP0027, "SIZE=(lines,columns), each 1 to 240"))?;
            match dims[..] {
                [l, c] => (l, c),
                _ => return Err(fail(st.line, crate::messages::IWP0027, "SIZE=(lines,columns), each 1 to 240")),
            }
        }
    };
    let origin = |key: &str| -> Result<u16, Error> {
        match ops.word(key)?.as_deref() {
            None | Some("NEXT" | "SAME") => Ok(1),
            Some(w) => w.parse().ok().filter(|n| (1..=240).contains(n)).ok_or_else(|| fail(st.line, crate::messages::IWP0028, format!("{key}={w} is not a number from 1 to 240"))),
        }
    };
    Ok(Map {
        name,
        lines,
        columns,
        line: origin("LINE")?,
        column: origin("COLUMN")?,
        ctrl: ops.words("CTRL").unwrap_or_else(|| open.set.ctrl.clone()),
        tioapfx: ops.yes_no("TIOAPFX")?.unwrap_or(open.tioapfx),
        dsatts: ops.dsatts()?.unwrap_or_else(|| open.dsatts.clone()),
        fields: Vec::new(),
    })
}

fn attrb(ops: &Operands) -> Result<Attrb, Error> {
    let mut a = Attrb { protection: Protection::Askip, numeric: false, intensity: Intensity::Norm, detectable: false, cursor: false, fset: false };
    let Some(words) = ops.words("ATTRB") else { return Ok(a) };
    a.protection = Protection::Unprot;
    for w in words {
        match w.as_str() {
            "ASKIP" => a.protection = Protection::Askip,
            "PROT" => a.protection = Protection::Prot,
            "UNPROT" => a.protection = Protection::Unprot,
            "NUM" => a.numeric = true,
            "BRT" => a.intensity = Intensity::Brt,
            "NORM" => a.intensity = Intensity::Norm,
            "DRK" => a.intensity = Intensity::Drk,
            "DET" => a.detectable = true,
            "IC" => a.cursor = true,
            "FSET" => a.fset = true,
            other => return Err(fail(ops.line, crate::messages::IWP0029, format!("ATTRB={other} is not an attribute"))),
        }
    }
    Ok(a)
}

fn hex_bytes(s: &str, line: u32) -> Result<Vec<u8>, Error> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err(fail(line, crate::messages::IWP0030, "XINIT takes an even number of hexadecimal digits"));
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| fail(line, crate::messages::IWP0031, "XINIT takes hexadecimal digits"))).collect()
}

fn field(st: &Statement, ops: &Operands, map: &Map) -> Result<Field, Error> {
    let name = st.label.as_ref().map(|_| name_of(st, "DFHMDF", MAX_NAME)).transpose()?;
    let line = st.line;
    let attrb = attrb(ops)?;
    let occurs = ops.number("OCCURS", 1, u16::MAX.into())?.unwrap_or(1) as u16;
    let group = ops.word("GRPNAME")?;
    if group.is_some() && (ops.get("OCCURS").is_some() || name.is_none()) {
        return Err(fail(line, crate::messages::IWP0032, "GRPNAME needs a labelled field and does not go with OCCURS"));
    }
    if group.as_ref().is_some_and(|g| g.len() > MAX_NAME) {
        return Err(fail(line, crate::messages::IWP0033, "GRPNAME is longer than 30 characters"));
    }
    let initial = match (ops.text("INITIAL")?.or(ops.text("GINIT")?), ops.text("XINIT")?) {
        (_, Some(x)) => Some(Initial::Bytes(hex_bytes(&x, line)?)),
        (Some(t), None) => Some(Initial::Text(t)),
        (None, None) => None,
    };
    let (picin, picout) = (ops.text("PICIN")?, ops.text("PICOUT")?);
    let from_data = match &initial {
        Some(Initial::Text(t)) => t.chars().count() as u32,
        Some(Initial::Bytes(b)) => b.len() as u32,
        None => 0,
    };
    let length = match ops.number("LENGTH", 0, 256)? {
        Some(0) if name.is_some() => return Err(fail(line, crate::messages::IWP0034, "LENGTH=0 is allowed only on an unlabelled field, where it delimits an input field")),
        Some(n) => n,
        None => picin.iter().chain(&picout).map(|p| picture_size(p)).max().unwrap_or(from_data),
    };
    if length > 256 || (length == 0 && name.is_some()) {
        return Err(fail(line, crate::messages::IWP0035, "LENGTH is missing, or is not from 1 to 256"));
    }
    let (line_no, column) = position(ops, map)?;
    let justify = ops.words("JUSTIFY").unwrap_or_default();
    for w in &justify {
        if !matches!(w.as_str(), "LEFT" | "RIGHT" | "BLANK" | "ZERO") {
            return Err(fail(line, crate::messages::IWP0036, format!("JUSTIFY={w}: LEFT or RIGHT, BLANK or ZERO")));
        }
    }
    let has = |w: &str| justify.iter().any(|j| j == w);
    let color = ops.word("COLOR")?;
    if let Some(c) = &color
        && !matches!(c.as_str(), "BLUE" | "RED" | "PINK" | "GREEN" | "TURQUOISE" | "YELLOW" | "NEUTRAL" | "DEFAULT")
    {
        return Err(fail(line, crate::messages::IWP0037, format!("COLOR={c} is not a colour")));
    }
    let hilight = ops.word("HILIGHT")?;
    if let Some(h) = &hilight
        && !matches!(h.as_str(), "OFF" | "BLINK" | "REVERSE" | "UNDERLINE")
    {
        return Err(fail(line, crate::messages::IWP0038, format!("HILIGHT={h} is not a highlight")));
    }
    Ok(Field {
        name,
        line: line_no,
        column,
        length: length as u16,
        attrb,
        initial,
        picin,
        picout,
        occurs,
        group,
        justify_right: if has("RIGHT") || has("LEFT") { has("RIGHT") } else { attrb.numeric },
        fill_zero: if has("ZERO") || has("BLANK") { has("ZERO") } else { attrb.numeric },
        color,
        hilight,
    })
}

/// POS as a 0-based offset or (line,column); without POS the field follows the previous one.
fn position(ops: &Operands, map: &Map) ->Result<(u16, u16), Error> {
    let bad = || fail(ops.line, crate::messages::IWP0039, "POS is an offset within the map, or (line,column) inside it");
    let (lines, columns) = (u32::from(map.lines), u32::from(map.columns));
    let offset = match ops.words("POS") {
        None => map.fields.last().map_or(0, |f| (u32::from(f.line) - 1) * columns + u32::from(f.column) - 1 + u32::from(f.occurs) * (u32::from(f.length) + 1)),
        Some(v) => match v.iter().map(|s| s.parse::<u32>()).collect::<Result<Vec<_>, _>>().map_err(|_| bad())?[..] {
            [n] => n,
            [l, c] if (1..=lines).contains(&l) && (1..=columns).contains(&c) => (l - 1) * columns + c - 1,
            _ => return Err(bad()),
        },
    };
    if offset >= lines * columns {
        return Err(bad());
    }
    Ok(((offset / columns + 1) as u16, (offset % columns + 1) as u16))
}

/// The COBOL a program COPYs for the mapset: each map's input and output structures.
pub fn symbolic_map(mapset: &Mapset) -> String {
    let mut out = String::new();
    for map in &mapset.maps {
        if mapset.mode != Mode::Out {
            structure(&mut out, map, true, mapset.mode);
        }
        if mapset.mode != Mode::In {
            structure(&mut out, map, false, mapset.mode);
        }
    }
    out
}

/// One COBOL data entry; an entry that would pass column 72 puts its clauses on the next line.
fn entry(out: &mut String, level: usize, name: &str, clauses: &str) {
    let indent = if level == 1 { 7 } else { 11 + 3 * (level - 2) };
    let sep = if level == 1 { "  " } else { " " };
    let head = format!("{:indent$}{level:02}{sep}{name}", "");
    let line = if clauses.is_empty() { format!("{head}.") } else { format!("{head} {clauses}.") };
    if line.len() <= 72 || clauses.is_empty() {
        let _ = writeln!(out, "{line}");
    } else {
        let _ = writeln!(out, "{head}\n{:width$}{clauses}.", "", width = indent + 4);
    }
}

fn structure(out: &mut String, map: &Map, input: bool, mode: Mode) {
    let side = if input { 'I' } else { 'O' };
    if input || mode == Mode::Out {
        entry(out, 1, &format!("{}{side}", map.name), "");
    } else {
        entry(out, 1, &format!("{}O", map.name), &format!("REDEFINES {}I", map.name));
    }
    if map.tioapfx {
        entry(out, 2, "FILLER", "PIC X(12)");
    }
    let mut group: Option<&str> = None;
    for f in map.fields.iter().filter(|f| f.name.is_some()) {
        let (level, lead) = match &f.group {
            Some(g) => {
                let first = group != Some(g.as_str());
                if first {
                    entry(out, 2, g, "");
                }
                group = Some(g);
                (3, first)
            }
            None => {
                group = None;
                if f.occurs > 1 {
                    let name = format!("{}{}", f.name.as_deref().unwrap_or_default(), if input { 'D' } else { 'G' });
                    entry(out, 2, &name, &format!("OCCURS {}", f.occurs));
                    (3, true)
                } else {
                    (2, true)
                }
            }
        };
        if input {
            input_items(out, map, f, level, lead);
        } else {
            output_items(out, map, f, level, lead, mode);
        }
    }
}

fn input_items(out: &mut String, map: &Map, f: &Field, level: usize, lead: bool) {
    let n = f.name.as_deref().unwrap_or_default();
    if lead {
        entry(out, level, &format!("{n}L"), "COMP PIC S9(4)");
        entry(out, level, &format!("{n}F"), "PICTURE X");
        entry(out, level, "FILLER", &format!("REDEFINES {n}F"));
        entry(out, level + 1, &format!("{n}A"), "PICTURE X");
        let k = extended_attributes(map).len();
        if k > 0 {
            entry(out, level, "FILLER", &format!("PICTURE X({k})"));
        }
    }
    let pic = f.picin.clone().unwrap_or_else(|| format!("X({})", f.length));
    entry(out, level, &format!("{n}I"), &format!("PIC {pic}"));
}

fn output_items(out: &mut String, map: &Map, f: &Field, level: usize, lead: bool, mode: Mode) {
    let n = f.name.as_deref().unwrap_or_default();
    if lead {
        if mode == Mode::InOut {
            entry(out, level, "FILLER", "PICTURE X(3)");
        } else {
            entry(out, level, "FILLER", "PICTURE X(2)");
            entry(out, level, &format!("{n}A"), "PICTURE X");
        }
        for (a, s, _) in EXTENDED {
            if map.dsatts.iter().any(|d| d == a) {
                entry(out, level, &format!("{n}{s}"), "PICTURE X");
            }
        }
    }
    let pic = f.picout.clone().unwrap_or_else(|| format!("X({})", f.length));
    entry(out, level, &format!("{n}O"), &format!("PIC {pic}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_match_the_symbolic_map() {
        let sets = parse(&realistic()).unwrap();
        let map = &sets[0].maps[0];
        let slots = slots(map, true);
        let k = extended_attributes(map).len();
        let first = slots[0];
        assert_eq!(first.length_at(), Some(if map.tioapfx { 12 } else { 0 }));
        assert_eq!(first.data, first.length_at().unwrap() + 3 + k);
        for pair in slots.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let gap = if b.control.is_some() { 3 + k } else { 0 };
            assert_eq!(b.data, a.data + a.size + gap);
        }
    }

    /// A line with X in column 72.
    fn cont(line: &str) -> String {
        format!("{line:<71}X\n")
    }

    fn realistic() -> String {
        let mut s = String::new();
        s.push_str("* Demo mapset\n         PRINT NOGEN\n");
        s.push_str(&cont("DEMOSET  DFHMSD TYPE=&SYSPARM,MODE=INOUT,LANG=COBOL,STORAGE=AUTO,"));
        s.push_str("               CTRL=(FREEKB,FRSET),DSATTS=(COLOR,HILIGHT)   SEQ00010\n");
        s.push_str("DEMO     DFHMDI SIZE=(24,80),LINE=1,COLUMN=1\n");
        s.push_str(&cont("         DFHMDF POS=(1,30),LENGTH=15,ATTRB=(ASKIP,BRT),"));
        s.push_str("               INITIAL='ACCOUNT ENQUIRY'\n");
        s.push_str(&cont("NAME     DFHMDF POS=(3,10),LENGTH=20,ATTRB=(UNPROT,IC,FSET),COLOR=RED,"));
        s.push_str("               HILIGHT=UNDERLINE,INITIAL='O''NEIL'\n");
        s.push_str(&cont("AMT      DFHMDF POS=165,ATTRB=(NUM,BRT),PICIN='9(5)V99',"));
        s.push_str("               PICOUT='ZZ,ZZ9.99',JUSTIFY=(RIGHT,ZERO)\n");
        s.push_str("ROW      DFHMDF POS=(6,2),LENGTH=8,OCCURS=3\n");
        s.push_str("ADDR1    DFHMDF POS=(10,2),LENGTH=20,GRPNAME=ADDR,ATTRB=(UNPROT)\n");
        s.push_str("ADDR2    DFHMDF POS=(11,2),LENGTH=20,GRPNAME=ADDR\n");
        s.push_str("         DFHMSD TYPE=FINAL\n         END\n");
        s
    }

    #[test]
    fn a_realistic_mapset_parses() {
        let sets = parse(&realistic()).unwrap();
        assert_eq!(sets.len(), 1);
        let set = &sets[0];
        assert_eq!((set.name.as_str(), set.mode), ("DEMOSET", Mode::InOut));
        assert_eq!(set.ctrl, ["FREEKB", "FRSET"]);
        let map = &set.maps[0];
        assert_eq!((map.name.as_str(), map.lines, map.columns), ("DEMO", 24, 80));
        assert!(map.tioapfx);
        assert_eq!(map.dsatts, ["COLOR", "HILIGHT"]);
        assert_eq!(map.fields.len(), 6);

        let title = &map.fields[0];
        assert_eq!((title.name.clone(), title.line, title.column, title.length), (None, 1, 30, 15));
        assert_eq!(title.initial, Some(Initial::Text("ACCOUNT ENQUIRY".into())));
        assert_eq!((title.attrb.protection, title.attrb.intensity), (Protection::Askip, Intensity::Brt));

        let name = &map.fields[1];
        assert_eq!(name.name.as_deref(), Some("NAME"));
        assert_eq!((name.line, name.column), (3, 10));
        assert!(name.attrb.cursor && name.attrb.fset && name.attrb.protection == Protection::Unprot);
        assert_eq!((name.color.as_deref(), name.hilight.as_deref()), (Some("RED"), Some("UNDERLINE")));
        assert_eq!(name.initial, Some(Initial::Text("O'NEIL".into())));
        assert!(!name.justify_right && !name.fill_zero);

        let amt = &map.fields[2];
        assert_eq!((amt.line, amt.column), (3, 6));
        assert_eq!((amt.picin.as_deref(), amt.picout.as_deref()), (Some("9(5)V99"), Some("ZZ,ZZ9.99")));
        assert_eq!(amt.length, 9);
        assert!(amt.attrb.numeric && amt.justify_right && amt.fill_zero);

        assert_eq!(map.fields[3].occurs, 3);
        assert_eq!(map.fields[4].group.as_deref(), Some("ADDR"));
        assert_eq!(map.fields[5].group.as_deref(), Some("ADDR"));
    }

    #[test]
    fn zero_length_is_for_an_unlabelled_field_only() {
        let src = |label: &str| format!("M DFHMSD TYPE=MAP\nM1 DFHMDI SIZE=(24,80)\n{label:<9}DFHMDF POS=(6,33),LENGTH=0,ATTRB=ASKIP\n");
        let map = &parse(&src("")).unwrap()[0].maps[0];
        assert_eq!((map.fields[0].name.clone(), map.fields[0].length), (None, 0));
        let err = parse(&src("AMT")).unwrap_err();
        assert!(err.to_string().contains("LENGTH=0 is allowed only on an unlabelled field"), "{err}");
    }

    #[test]
    fn a_quoted_string_continues_on_the_next_line() {
        let head = "         DFHMDF POS=1,LENGTH=20,INITIAL='";
        let fill = "A".repeat(71 - head.len());
        let src = format!("M DFHMSD TYPE=MAP\nM1 DFHMDI\n{}{}", cont(&format!("{head}{fill}")), "               BC'\n");
        let map = &parse(&src).unwrap()[0].maps[0];
        assert_eq!(map.fields[0].initial, Some(Initial::Text(format!("{fill}BC"))));
    }

    #[test]
    fn xinit_gives_bytes_and_several_mapsets_are_kept() {
        let src = "S1 DFHMSD TYPE=MAP\nM1 DFHMDI\n   DFHMDF POS=1,LENGTH=2,XINIT=C1C2\n   DFHMSD TYPE=FINAL\nS2 DFHMSD TYPE=MAP,MODE=IN\n   DFHMSD TYPE=FINAL\n";
        let sets = parse(src).unwrap();
        assert_eq!(sets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["S1", "S2"]);
        assert_eq!(sets[0].maps[0].fields[0].initial, Some(Initial::Bytes(vec![0xC1, 0xC2])));
        assert_eq!(sets[1].mode, Mode::In);
        assert_eq!(sets[0].mode, Mode::Out);
    }

    fn two_field_map() -> Mapset {
        let src = "MAPS DFHMSD TYPE=MAP,MODE=INOUT,STORAGE=AUTO,DSATTS=(COLOR)\nMAP1 DFHMDI SIZE=(24,80)\nNAME DFHMDF POS=(1,1),LENGTH=10\nAMT DFHMDF POS=(2,1),PICIN='9(5)',PICOUT='ZZZZ9'\n DFHMSD TYPE=FINAL\n";
        parse(src).unwrap().remove(0)
    }

    #[test]
    fn the_symbolic_map_of_a_two_field_map() {
        let expected = "\n       01  MAP1I.
           02 FILLER PIC X(12).
           02 NAMEL COMP PIC S9(4).
           02 NAMEF PICTURE X.
           02 FILLER REDEFINES NAMEF.
              03 NAMEA PICTURE X.
           02 FILLER PICTURE X(1).
           02 NAMEI PIC X(10).
           02 AMTL COMP PIC S9(4).
           02 AMTF PICTURE X.
           02 FILLER REDEFINES AMTF.
              03 AMTA PICTURE X.
           02 FILLER PICTURE X(1).
           02 AMTI PIC 9(5).
       01  MAP1O REDEFINES MAP1I.
           02 FILLER PIC X(12).
           02 FILLER PICTURE X(3).
           02 NAMEC PICTURE X.
           02 NAMEO PIC X(10).
           02 FILLER PICTURE X(3).
           02 AMTC PICTURE X.
           02 AMTO PIC ZZZZ9.
";
        assert_eq!(symbolic_map(&two_field_map()).lines().collect::<Vec<_>>(), expected[1..].lines().collect::<Vec<_>>());
    }

    #[test]
    fn output_only_occurs_and_groups() {
        let src = "MAPS DFHMSD TYPE=MAP,TIOAPFX=NO\nM1 DFHMDI\nROW DFHMDF POS=1,LENGTH=4,OCCURS=2\nA1 DFHMDF POS=100,LENGTH=3,GRPNAME=G\nA2 DFHMDF POS=120,LENGTH=5,GRPNAME=G\n DFHMSD TYPE=FINAL\n";
        let mapset = parse(src).unwrap().remove(0);
        let expected = "\n       01  M1O.
           02 ROWG OCCURS 2.
              03 FILLER PICTURE X(2).
              03 ROWA PICTURE X.
              03 ROWO PIC X(4).
           02 G.
              03 FILLER PICTURE X(2).
              03 A1A PICTURE X.
              03 A1O PIC X(3).
              03 A2O PIC X(5).
";
        assert_eq!(symbolic_map(&mapset), &expected[1..]);
        let input = symbolic_map(&Mapset { mode: Mode::In, ..mapset });
        assert!(input.contains("           02 ROWD OCCURS 2.\n              03 ROWL COMP PIC S9(4).\n"), "{input}");
        assert!(input.contains("           02 G.\n              03 A1L COMP PIC S9(4).") && input.contains("              03 A2I PIC X(5).\n"), "{input}");
        assert!(!input.contains("A2L") && input.lines().all(|l| l.len() <= 72));
    }

    fn library(name: &str, files: &[(&str, String)]) -> Libraries {
        let dir = std::env::temp_dir().join(format!("ironwork-bms-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            std::fs::write(dir.join(file), text).unwrap();
        }
        Libraries::new(vec![dir])
    }

    /// Offsets of each level-02 item under the 01 `record` of the working storage.
    fn offsets(program: &crate::ast::Program, record: &str) -> Vec<(String, u32)> {
        let mut at = 0;
        let mut inside = false;
        let mut parent = 0;
        let mut out: Vec<(String, u32)> = Vec::new();
        for e in &program.working_storage {
            match e.level {
                1 => inside = e.name.as_deref() == Some(record),
                2 if inside => {
                    let size = if e.usage.is_some() { 2 } else { e.picture.as_deref().map_or(0, picture_size) };
                    let name = e.name.clone().unwrap_or_default();
                    match &e.redefines {
                        Some(target) => {
                            parent = out.iter().find(|(n, _)| n == target).unwrap().1;
                            out.push((name, parent));
                        }
                        None => {
                            parent = at;
                            out.push((name, at));
                            at += size;
                        }
                    }
                }
                3 if inside => out.push((e.name.clone().unwrap_or_default(), parent)),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn a_program_copies_a_mapset_and_gets_its_layout() {
        let bms = "DEMOMAP DFHMSD TYPE=&SYSPARM,MODE=INOUT,STORAGE=AUTO\nM1 DFHMDI SIZE=(24,80)\nNAME DFHMDF POS=(1,1),LENGTH=10\n DFHMSD TYPE=FINAL\n".to_owned();
        let libs = library("layout", &[("DEMOMAP.bms", bms)]);
        let program = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n           COPY DEMOMAP.\n       PROCEDURE DIVISION.\n           GOBACK.\n";
        let p = crate::parse_with(program, &libs).unwrap();
        let at = |n: &str| offsets(&p, "M1I").into_iter().find(|(name, _)| name == n).unwrap().1;
        assert_eq!((at("NAMEL"), at("NAMEF"), at("NAMEA"), at("NAMEI")), (12, 14, 14, 15));
        assert!(p.working_storage.iter().any(|e| e.name.as_deref() == Some("M1O") && e.redefines.as_deref() == Some("M1I")));
        assert_eq!(find_mapset(&libs, "DEMOMAP").unwrap().unwrap().maps[0].fields[0].name.as_deref(), Some("NAME"));
        assert!(find_mapset(&libs, "NOPE").is_none());
    }

    #[test]
    fn copy_of_a_broken_mapset_names_the_file() {
        let libs = library("broken", &[("BAD.bms", "BAD DFHMSD TYPE=MAP\n FROBNICATE X=1\n".to_owned())]);
        let err = crate::parse_with("       COPY BAD.\n", &libs).unwrap_err();
        assert!(err.message.contains("unknown macro FROBNICATE") && err.message.contains("BAD.bms"), "{err}");
    }

    #[test]
    fn dfhaid_has_values() {
        let src = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n           COPY DFHAID.\n           COPY DFHBMSCA.\n       PROCEDURE DIVISION.\n           GOBACK.\n";
        let p = crate::parse_with(src, &Libraries::default()).unwrap();
        let entry = |n: &str| p.working_storage.iter().find(|e| e.name.as_deref() == Some(n)).unwrap();
        assert_eq!(entry("DFHENTER").value, Some(crate::ast::Literal::Hex(vec![0x7D])));
        assert_eq!(entry("DFHPF24").value, Some(crate::ast::Literal::Hex(vec![0x4C])));
        assert_eq!(entry("DFHBMASK").value, Some(crate::ast::Literal::Hex(vec![0xF0])));
        let erase = entry("DFHERASE");
        assert_eq!((erase.level, erase.condition_values.len()), (88, 2));
    }

    #[test]
    fn errors_point_at_the_line() {
        let e = |src: &str| parse(src).unwrap_err();
        assert!(e("S DFHMSD TYPE=MAP\n WIBBLE\n").message.contains("unknown macro WIBBLE"));
        let outside = e("* c\nF DFHMDF POS=1,LENGTH=1\n");
        assert_eq!(outside.pos.line, 2);
        assert!(outside.message.contains("outside"));
        let bad = e("S DFHMSD TYPE=MAP\nM DFHMDI SIZE=(2,10)\nF DFHMDF POS=(3,1),LENGTH=1\n");
        assert_eq!(bad.pos.line, 3);
        assert!(bad.message.contains("POS"));
        assert!(e("S DFHMSD TYPE=MAP\nM DFHMDI\nF DFHMDF POS=ABC,LENGTH=1\n").message.contains("POS"));
        assert!(e("S DFHMSD TYPE=MAP\nTOOLONGNAME DFHMDI\n").message.contains("longer than 7"));
        assert!(e("S DFHMSD TYPE=MAP\nM DFHMDI\nF DFHMDF POS=1,LENGTH=1,OCCURS=2,GRPNAME=G\n").message.contains("GRPNAME"));
    }
}
