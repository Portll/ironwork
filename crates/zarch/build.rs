use std::{env, fmt::Write as _, fs, path::Path};

struct Page {
    ccsid: u16,
    name: String,
    decode: [u32; 256],
    encode: Vec<(u32, u8)>,
    dbcs: Option<Dbcs>,
}

/// A mixed page's two-byte characters, the ones between shift-out and shift-in.
#[derive(PartialEq, Eq)]
struct Dbcs {
    /// Each code and its character, sorted by code; `ONE_WAY` marks one that does not round-trip.
    decode: Vec<(u16, u32)>,
    /// Codes whose character is two code points, both ways.
    sequences: Vec<(u16, u32, u32)>,
    /// Characters encoded to a code that decodes to another character.
    encode_only: Vec<(u32, u16)>,
}

const ONE_WAY: u32 = 1 << 31;

/// What IBM's conversions give for a single byte with no character in the page.
const SUB: u32 = 0x1A;

const SHIFT_OUT: u8 = 0x0E;
const SHIFT_IN: u8 = 0x0F;

/// The DBCS component of each mixed CCSID (Programming Guide SC27-8714-03, Table 47).
const DBCS_COMPONENTS: [(u16, u16); 11] =
    [(930, 300), (939, 300), (5026, 4396), (5035, 4396), (1390, 16684), (1399, 16684), (933, 834), (1364, 4930), (935, 837), (1388, 4933), (937, 835)];

fn main() {
    println!("cargo::rerun-if-changed=ucm");
    let mut paths: Vec<_> = fs::read_dir("ucm")
        .expect("ucm directory")
        .map(|e| e.expect("ucm entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ucm"))
        .collect();
    paths.sort();
    let mut pages: Vec<Page> = paths.iter().map(|p| read_ucm(p)).collect();
    pages.sort_by_key(|p| p.ccsid);

    let out_dir = env::var("OUT_DIR").unwrap();
    let mut tables: Vec<&Dbcs> = Vec::new();
    let mut table_of = Vec::new();
    for page in &pages {
        table_of.push(page.dbcs.as_ref().map(|d| match tables.iter().position(|t| *t == d) {
            Some(k) => k,
            None => {
                tables.push(d);
                tables.len() - 1
            }
        }));
    }

    let mut out = String::new();
    for (k, table) in tables.iter().enumerate() {
        let blob: Vec<u8> = table.decode.iter().flat_map(|&(code, cp)| code.to_be_bytes().into_iter().chain(cp.to_be_bytes())).collect();
        fs::write(Path::new(&out_dir).join(format!("dbcs{k}.bin")), blob).unwrap();
        write!(out, "static DBCS{k}: Dbcs = Dbcs {{ decode: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/dbcs{k}.bin\")), sequences: &[").unwrap();
        for (code, a, b) in &table.sequences {
            write!(out, "(0x{code:04X}, ['\\u{{{a:04X}}}', '\\u{{{b:04X}}}']),").unwrap();
        }
        write!(out, "], encode_only: &[").unwrap();
        for (cp, code) in &table.encode_only {
            write!(out, "('\\u{{{cp:04X}}}', 0x{code:04X}),").unwrap();
        }
        writeln!(out, "], encode: OnceLock::new() }};").unwrap();
    }
    writeln!(out, "static PAGES: [CodePage; {}] = [", pages.len()).unwrap();
    for (page, table) in pages.iter().zip(&table_of) {
        writeln!(out, "CodePage {{ ccsid: {}, name: {:?}, decode: [", page.ccsid, page.name).unwrap();
        for cp in page.decode {
            write!(out, "'\\u{{{cp:04X}}}',").unwrap();
        }
        writeln!(out, "], encode: &[").unwrap();
        for (cp, byte) in &page.encode {
            write!(out, "('\\u{{{cp:04X}}}', 0x{byte:02X}),").unwrap();
        }
        match table {
            Some(k) => {
                let component = DBCS_COMPONENTS.iter().find(|(mixed, _)| *mixed == page.ccsid).map(|&(_, c)| c);
                let component = component.unwrap_or_else(|| panic!("{}: a mixed page Table 47 does not list", page.name));
                writeln!(out, "], dbcs: Some(({component}, &DBCS{k})) }},").unwrap();
            }
            None => writeln!(out, "], dbcs: None }},").unwrap(),
        }
    }
    writeln!(out, "];").unwrap();
    fs::write(Path::new(&out_dir).join("codepages.rs"), out).unwrap();
}

/// A code point list as ICU writes it, `<U3042>` or `<U304B><U309A>`.
fn code_points(name: &str, field: &str) -> Vec<u32> {
    field
        .split('<')
        .filter(|s| !s.is_empty())
        .map(|s| u32::from_str_radix(s.trim_start_matches('U').trim_end_matches('>'), 16).unwrap_or_else(|_| panic!("{name}: {field}")))
        .collect()
}

fn hex_bytes(name: &str, field: &str) -> Vec<u8> {
    field.split("\\x").filter(|s| !s.is_empty()).map(|s| u8::from_str_radix(s, 16).unwrap_or_else(|_| panic!("{name}: {field}"))).collect()
}

// ICU .ucm: |0 maps both ways, |1 only Unicode to bytes, |3 only bytes to Unicode, |2 a
// substitution fallback from Unicode, which IBM's tables do not carry.
fn read_ucm(path: &Path) -> Page {
    let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
    let ccsid = name
        .strip_prefix("ibm-")
        .and_then(|s| s.split('_').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("{name}: no CCSID in the file name"));
    let text = fs::read_to_string(path).unwrap();
    let mixed = text.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == ["<mb_cur_max>", "2"]);
    let mut decode = [None::<u32>; 256];
    let mut encode = Vec::new();
    let mut dbcs = Dbcs { decode: Vec::new(), sequences: Vec::new(), encode_only: Vec::new() };
    let mut in_map = false;
    for line in text.lines().map(str::trim) {
        match line {
            "CHARMAP" => in_map = true,
            "END CHARMAP" => break,
            _ if !in_map || line.is_empty() || line.starts_with('#') => {}
            _ => {
                let mut fields = line.split_whitespace();
                let (cps, bytes, flag) = (fields.next().unwrap(), fields.next().unwrap(), fields.next().unwrap());
                let (cps, bytes) = (code_points(&name, cps), hex_bytes(&name, bytes));
                match (bytes.as_slice(), cps.as_slice(), flag) {
                    (_, _, "|2") if mixed => {}
                    (&[byte], &[cp], "|0") => {
                        assert!(decode[byte as usize].replace(cp).is_none(), "{name}: byte {byte:02X} mapped twice");
                        encode.push((cp, byte));
                    }
                    (&[byte], &[cp], "|1") => encode.push((cp, byte)),
                    (&[byte], &[cp], "|3") => {
                        decode[byte as usize].get_or_insert(cp);
                    }
                    (&[hi, lo], &[cp], "|0" | "|3") if mixed => dbcs.decode.push((u16::from_be_bytes([hi, lo]), if flag == "|3" { cp | ONE_WAY } else { cp })),
                    (&[hi, lo], &[cp], "|1") if mixed => dbcs.encode_only.push((cp, u16::from_be_bytes([hi, lo]))),
                    (&[hi, lo], &[a, b], "|0") if mixed => dbcs.sequences.push((u16::from_be_bytes([hi, lo]), a, b)),
                    _ => panic!("{name}: a mapping this build does not read: {line}"),
                }
            }
        }
    }
    encode.sort_unstable();
    for pair in encode.windows(2) {
        assert!(pair[0].0 != pair[1].0, "{name}: U+{:04X} encoded twice", pair[0].0);
    }
    if !mixed {
        let decode = decode.map(|d| d.unwrap_or_else(|| panic!("{name}: a byte with no mapping")));
        return Page { ccsid, name, decode, encode, dbcs: None };
    }
    let decode: [u32; 256] = std::array::from_fn(|b| match b as u8 {
        SHIFT_OUT | SHIFT_IN => b as u32,
        _ => decode[b].unwrap_or(SUB),
    });
    dbcs.decode.sort_unstable();
    for pair in dbcs.decode.windows(2) {
        assert!(pair[0].0 != pair[1].0, "{name}: code {:04X} mapped twice", pair[0].0);
    }
    dbcs.sequences.sort_unstable();
    dbcs.encode_only.sort_unstable();
    Page { ccsid, name, decode, encode, dbcs: Some(dbcs) }
}
