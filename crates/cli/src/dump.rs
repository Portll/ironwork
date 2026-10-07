//! `ironwork dump`: a load module as text, one fact per line, in section order (load-module.md §11).

use exec::lir::{Debug as DebugTable, Listing, ProgramOptions, SqlEntry};
use exec::module::codec::decode_all;
use exec::module::crc::crc32;
use exec::module::{DirectoryEntry, LayoutRecord, LirRecord, LirRecords, Module, ModuleError, OptionRecords, Section, SectionEntry, SourceFile, StringTable};
use rt::bms::Mapset;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

pub struct Request {
    pub file: PathBuf,
    pub options: Options,
}

#[derive(Default)]
pub struct Options {
    /// The sections to print; all of them when empty.
    pub only: Vec<Section>,
    pub strings: bool,
    /// False under `--no-check`: a section whose checksum differs is printed, and does not fail the dump.
    pub check: bool,
}

pub fn section_named(name: &str) -> Option<Section> {
    Section::ALL.into_iter().chain([exec::module::FACTS]).find(|s| s.name.eq_ignore_ascii_case(name))
}

pub fn run(r: Request) -> ExitCode {
    let shown = r.file.display();
    let bytes = match std::fs::read(&r.file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ironwork: {shown}: {e}");
            return ExitCode::from(2);
        }
    };
    match dump(&bytes, &r.options) {
        Ok((text, sound)) => {
            print!("{text}");
            if sound { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Err(e) => {
            eprintln!("ironwork: {shown}: {e}");
            ExitCode::from(1)
        }
    }
}

/// A section's records, or why they are not printed.
type Decoded<T> = Result<Vec<T>, String>;

/// The text, and whether the module is sound: every checksum matched (unless not checked), every
/// section decoded, and the reader takes it. Fails only where the header or section table does.
pub fn dump(bytes: &[u8], o: &Options) -> Result<(String, bool), ModuleError> {
    let module = Module::read(bytes)?;
    let mut out = String::new();
    let mut sound = true;
    let version = module.version();
    let _ = writeln!(out, "format {}.{}", version.major, version.minor);
    let _ = writeln!(out, "length {}", bytes.len());
    let mut bodies: Vec<(SectionEntry, &[u8], bool)> = Vec::new();
    for entry in module.sections() {
        let body = usize::try_from(entry.offset)
            .ok()
            .zip(usize::try_from(entry.length).ok())
            .and_then(|(at, len)| bytes.get(at..at.checked_add(len)?))
            .unwrap_or_default();
        let computed = crc32(body);
        let matched = computed == entry.crc;
        let name = entry.name().or((entry.id == exec::module::FACTS.id).then_some(exec::module::FACTS.name)).map_or_else(|| format!("{:#x}", entry.id), str::to_owned);
        let optional = if entry.optional() { " optional" } else { "" };
        let state = if matched { "ok".to_owned() } else { format!("CHECKSUM MISMATCH (computed {computed:08X})") };
        let _ = writeln!(out, "section {} {name}{optional} offset {} length {} crc {:08X} {state}", entry.id, entry.offset, entry.length, entry.crc);
        sound &= matched || !o.check;
        bodies.push((*entry, body, matched));
    }
    let body = |s: Section| -> Result<&[u8], String> {
        match bodies.iter().find(|(e, _, _)| e.id == s.id) {
            Some((_, body, matched)) if *matched || !o.check => Ok(body),
            Some(_) => Err(format!("{} not printed: CHECKSUM MISMATCH", s.name)),
            None => Err(format!("{} is missing", s.name)),
        }
    };
    let strings = body(Section::STRINGS).and_then(|b| StringTable::decode(b).map_err(|e| e.to_string()));
    let table = strings.clone().unwrap_or_default();
    fn records<T: exec::module::Decode>(body: Result<&[u8], String>, s: Section, strings: &StringTable) -> Decoded<T> {
        body.and_then(|b| decode_all::<Vec<T>>(s.name, b, strings).map_err(|e| e.to_string()))
    }
    let directory: Decoded<DirectoryEntry> = records(body(Section::DIRECTORY), Section::DIRECTORY, &table);
    let options: Decoded<ProgramOptions> = body(Section::OPTIONS).and_then(|b| decode_all::<OptionRecords>(Section::OPTIONS.name, b, &table).map(|o| o.0).map_err(|e| e.to_string()));
    let layout: Decoded<LayoutRecord> = records(body(Section::LAYOUT), Section::LAYOUT, &table);
    let lir: Decoded<LirRecord> = body(Section::LIR).and_then(|b| decode_all::<LirRecords>(Section::LIR.name, b, &table).map(|l| l.0).map_err(|e| e.to_string()));
    let sql: Decoded<Vec<SqlEntry>> = records(body(Section::SQL), Section::SQL, &table);
    let bms: Result<Vec<Mapset>, String> = body(Section::BMS).and_then(|b| decode_all::<Vec<Mapset>>(Section::BMS.name, b, &table).map_err(|e| e.to_string()));
    let debug: Decoded<(DebugTable, Vec<Option<SourceFile>>)> = records(body(Section::DEBUG), Section::DEBUG, &table);

    let count = directory.as_ref().map_or(0, Vec::len);
    let names: Vec<String> = match &directory {
        Ok(d) => d.iter().map(|e| e.id.clone()).collect(),
        Err(_) => Vec::new(),
    };
    let name = |k: usize| names.get(k).cloned().unwrap_or_else(|| format!("program{k}"));
    let symbols = |k: usize| lir.as_ref().ok().and_then(|l| l.get(k)).map(|l| l.symbols.as_slice());
    let shows = |s: Section| if o.only.is_empty() { s != Section::STRINGS || o.strings } else { o.only.contains(&s) };
    let mut failed = |out: &mut String, why: &str| {
        let _ = writeln!(out, "{why}");
        sound = false;
    };

    if shows(Section::STRINGS) {
        let _ = writeln!(out, "STRINGS");
        match &strings {
            Ok(t) => t.iter().enumerate().for_each(|(k, s)| {
                let _ = writeln!(out, "string {k} {s:?}");
            }),
            Err(e) => failed(&mut out, e),
        }
    } else if let Err(e) = &strings {
        failed(&mut out, e);
    }
    let per_program = |s: Section, n: usize| (directory.is_ok() && n != count).then(|| format!("{} holds {n} records for {count} programs", s.name));

    if shows(Section::DIRECTORY) {
        let _ = writeln!(out, "DIRECTORY");
        match &directory {
            Ok(d) => {
                for (k, e) in d.iter().enumerate() {
                    let parent = e.parent.map_or_else(|| "-".to_owned(), |p| p.to_string());
                    let params: Vec<&str> = e.params.iter().map(|&v| if v { "value" } else { "reference" }).collect();
                    let _ = writeln!(
                        out,
                        "program {k} {} parent {parent} common {} dynamic {} using [{}] returning {}",
                        e.id,
                        yes(e.common),
                        yes(e.dynamic),
                        params.join(" "),
                        yes(e.returning)
                    );
                    if let Some(external) = &e.external {
                        let _ = writeln!(out, "program {k} external {external}");
                    }
                    for (entry, paragraph) in &e.entries {
                        let _ = writeln!(out, "program {k} entry {entry} paragraph {paragraph}");
                    }
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    if shows(Section::OPTIONS) {
        let _ = writeln!(out, "OPTIONS");
        match &options {
            Ok(all) => {
                if let Some(why) = per_program(Section::OPTIONS, all.len()) {
                    failed(&mut out, &why);
                }
                for (k, p) in all.iter().enumerate() {
                    let program = name(k);
                    for field in parts(&format!("{:?}", p.options)).into_iter().filter(|f| !f.starts_with("assumed: ")) {
                        let _ = writeln!(out, "{program} {field}");
                    }
                    for flag in p.options.assume_flags() {
                        let _ = writeln!(out, "{program} assume {}", &flag["--assume=".len()..]);
                    }
                    for field in parts(&format!("{p:?}")).into_iter().filter(|f| !f.starts_with("options: ")) {
                        let _ = writeln!(out, "{program} {field}");
                    }
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    if shows(Section::LAYOUT) {
        let _ = writeln!(out, "LAYOUT");
        match &layout {
            Ok(all) => {
                if let Some(why) = per_program(Section::LAYOUT, all.len()) {
                    failed(&mut out, &why);
                }
                for (k, (storage, items, edits)) in all.iter().enumerate() {
                    let program = name(k);
                    let sym = |id: u32| symbol(symbols(k), id);
                    for field in parts(&format!("{storage:?}")) {
                        let shown = match field.split_once(": ") {
                            Some(("image", _)) => format!("image: {}", hex(&storage.image)),
                            Some(("local_image", _)) => format!("local_image: {}", hex(&storage.local_image)),
                            _ => field.to_owned(),
                        };
                        let _ = writeln!(out, "{program} storage {shown}");
                    }
                    for (i, item) in items.iter().enumerate() {
                        let odo = item.depending_on.map_or_else(|| "-".to_owned(), |d| format!("item {d}"));
                        let keys: Vec<String> = item.keys.iter().map(|(asc, key)| format!("{} item {key}", if *asc { "ascending" } else { "descending" })).collect();
                        let _ = writeln!(
                            out,
                            "{program} item {i} level {:02} name {} offset {} size {} occurs {} kind {:?} depending on {odo} keys [{}]",
                            item.level,
                            item.name.map_or_else(|| "FILLER".to_owned(), sym),
                            item.offset,
                            item.size,
                            item.occurs,
                            item.kind,
                            keys.join(", ")
                        );
                    }
                    for (i, edit) in edits.iter().enumerate() {
                        let _ = writeln!(out, "{program} edit {i} {edit:?}");
                    }
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    if shows(Section::LIR) {
        let _ = writeln!(out, "LIR");
        match &lir {
            Ok(all) => {
                if let Some(why) = per_program(Section::LIR, all.len()) {
                    failed(&mut out, &why);
                }
                for (k, l) in all.iter().enumerate() {
                    let listing = Listing {
                        code: l.code(),
                        items: layout.as_ref().ok().and_then(|all| all.get(k)).map_or(&[], |(_, items, _)| items),
                        debug: debug.as_ref().ok().and_then(|all| all.get(k)).map(|(d, _)| d),
                        sql: sql.as_ref().ok().and_then(|all| all.get(k)).map_or(&[], Vec::as_slice),
                        ccsid: options.as_ref().ok().and_then(|all| all.get(k)).map(|o| o.options.codepage),
                    };
                    let _ = write!(out, "{listing}");
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    if shows(Section::SQL) {
        let _ = writeln!(out, "SQL");
        match &sql {
            Ok(all) => {
                if let Some(why) = per_program(Section::SQL, all.len()) {
                    failed(&mut out, &why);
                }
                for (k, entries) in all.iter().enumerate() {
                    let sym = |id: u32| symbol(symbols(k), id);
                    for e in entries {
                        let identity = format!("{}:{}:{:08x}", name(k), e.ordinal, e.fingerprint);
                        let text = sym(e.text);
                        let _ = writeln!(out, "{identity} {}", if text.is_empty() { sym(e.verb) } else { text });
                        let _ = writeln!(out, "{identity} statement {:?}", e.statement);
                        if e.with_hold {
                            let _ = writeln!(out, "{identity} with hold");
                        }
                    }
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    if shows(Section::BMS) {
        let _ = writeln!(out, "BMS");
        match &bms {
            Ok(all) => {
                let _ = writeln!(out, "mapsets {}", all.len());
                for set in all {
                    let _ = writeln!(out, "mapset {} mode {:?} ctrl [{}] maps {}", set.name, set.mode, set.ctrl.join(" "), set.maps.len());
                    for map in &set.maps {
                        let _ = writeln!(
                            out,
                            "mapset {} map {} size {}x{} at {},{} ctrl [{}] tioapfx {} dsatts [{}]",
                            set.name,
                            map.name,
                            map.lines,
                            map.columns,
                            map.line,
                            map.column,
                            map.ctrl.join(" "),
                            yes(map.tioapfx),
                            map.dsatts.join(" ")
                        );
                        for (k, field) in map.fields.iter().enumerate() {
                            let _ = writeln!(out, "mapset {} map {} field {k} {field:?}", set.name, map.name);
                        }
                    }
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    if shows(Section::DEBUG) {
        let _ = writeln!(out, "DEBUG");
        match &debug {
            Ok(all) => {
                if let Some(why) = per_program(Section::DEBUG, all.len()) {
                    failed(&mut out, &why);
                }
                for (k, (d, read)) in all.iter().enumerate() {
                    let program = name(k);
                    let files: Vec<String> = d.sources.iter().map(|&s| symbol(symbols(k), s)).collect();
                    for (i, file) in files.iter().enumerate() {
                        let _ = writeln!(out, "{program} source {i} {file}");
                    }
                    for (i, file) in read.iter().enumerate() {
                        match file {
                            Some(f) => {
                                let _ = writeln!(out, "{program} file {i} root {} {} sha256 {} bytes {}", f.root, f.path, exec::digest::hex(&f.sha256), f.bytes);
                            }
                            None => {
                                let _ = writeln!(out, "{program} file {i} -");
                            }
                        }
                    }
                    for (i, pos) in d.positions.iter().enumerate() {
                        let file = files.get(usize::from(pos.file)).cloned().unwrap_or_else(|| format!("file{}", pos.file));
                        let _ = writeln!(out, "{program} #{i} {file}:{}:{}", pos.line, pos.col);
                    }
                    for (b, ops) in d.ops.iter().enumerate() {
                        let ids: Vec<String> = ops.iter().map(|id| format!("#{id}")).collect();
                        let _ = writeln!(out, "{program} block {b} {}", ids.join(" "));
                    }
                }
            }
            Err(e) => failed(&mut out, e),
        }
    }

    let read = exec::module::read(bytes);
    if let Ok(module) = &read
        && shows(exec::module::FACTS)
        && module.facts.iter().any(Option::is_some)
    {
        let _ = writeln!(out, "FACTS");
        for (k, facts) in module.facts.iter().enumerate() {
            let program = name(k);
            match facts {
                Some(f) => {
                    let statements: Vec<&str> = f.statements().map(|s| s.name()).collect();
                    let usages: Vec<&str> = f.usages().map(|u| u.name()).collect();
                    let _ = writeln!(out, "{program} statements [{}] usages [{}]", statements.join(" "), usages.join(" "));
                }
                None => {
                    let _ = writeln!(out, "{program} -");
                }
            }
        }
    }
    match read {
        Ok(_) => {
            let _ = writeln!(out, "module reads");
        }
        Err(e) => {
            let _ = writeln!(out, "module refused: {e}");
            sound &= !o.check;
        }
    }
    Ok((out, sound))
}

fn symbol(symbols: Option<&[String]>, id: u32) -> String {
    symbols.and_then(|s| s.get(id as usize)).cloned().unwrap_or_else(|| format!("symbol{id}"))
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_owned();
    }
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02X}");
        s
    })
}

/// The top-level parts of a `Debug` rendering: the fields of `Name { a: x, b: y }`, or the elements
/// of `[x, y]`, split at the commas outside brackets, strings and characters.
fn parts(text: &str) -> Vec<&str> {
    let inner = if let Some(list) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        list
    } else if let (Some(open), true) = (text.find('{'), text.ends_with('}')) {
        &text[open + 1..text.len() - 1]
    } else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    let mut chars = inner.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                while let Some((_, d)) = chars.next() {
                    match d {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '\'' => {
                if let Some((_, '\\')) = chars.next() {
                    chars.next();
                }
                for (_, d) in chars.by_ref() {
                    if d == '\'' {
                        break;
                    }
                }
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parts;

    #[test]
    fn a_debug_rendering_splits_at_its_top_level_commas() {
        assert_eq!(parts("Plans { arith: [A { x: 1, y: (2, 3) }], init: [] }"), ["arith: [A { x: 1, y: (2, 3) }]", "init: []"]);
        assert_eq!(parts(r#"[Text("a, \"b\""), Char(','), Char('\''), Char('\u{301}'), B]"#), [r#"Text("a, \"b\"")"#, "Char(',')", r"Char('\'')", r"Char('\u{301}')", "B"]);
        assert_eq!(parts("[]"), Vec::<&str>::new());
        assert_eq!(parts("Unit"), Vec::<&str>::new());
    }
}
