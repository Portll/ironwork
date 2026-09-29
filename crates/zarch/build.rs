use std::{env, fmt::Write as _, fs, path::Path};

struct Page {
    ccsid: u16,
    name: String,
    decode: [u32; 256],
    encode: Vec<(u32, u8)>,
}

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

    let mut out = String::new();
    writeln!(out, "static PAGES: [CodePage; {}] = [", pages.len()).unwrap();
    for page in &pages {
        writeln!(out, "CodePage {{ ccsid: {}, name: {:?}, decode: [", page.ccsid, page.name).unwrap();
        for cp in page.decode {
            write!(out, "'\\u{{{cp:04X}}}',").unwrap();
        }
        writeln!(out, "], encode: &[").unwrap();
        for (cp, byte) in &page.encode {
            write!(out, "('\\u{{{cp:04X}}}', 0x{byte:02X}),").unwrap();
        }
        writeln!(out, "] }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("codepages.rs");
    fs::write(dest, out).unwrap();
}

// ICU .ucm: |0 maps both ways, |1 only Unicode to bytes, |3 only bytes to Unicode.
fn read_ucm(path: &Path) -> Page {
    let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
    let ccsid = name
        .strip_prefix("ibm-")
        .and_then(|s| s.split('_').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("{name}: no CCSID in the file name"));
    let text = fs::read_to_string(path).unwrap();
    let mut decode = [None::<u32>; 256];
    let mut encode = Vec::new();
    let mut in_map = false;
    for line in text.lines().map(str::trim) {
        match line {
            "CHARMAP" => in_map = true,
            "END CHARMAP" => break,
            _ if !in_map || line.is_empty() || line.starts_with('#') => {}
            _ => {
                let mut fields = line.split_whitespace();
                let (cp, bytes, flag) = (fields.next().unwrap(), fields.next().unwrap(), fields.next().unwrap());
                let cp = u32::from_str_radix(cp.trim_start_matches("<U").trim_end_matches('>'), 16)
                    .unwrap_or_else(|_| panic!("{name}: {line}"));
                let byte = u8::from_str_radix(bytes.strip_prefix("\\x").unwrap_or_else(|| panic!("{name}: {line}")), 16)
                    .unwrap_or_else(|_| panic!("{name}: multi-byte mapping in a single-byte page: {line}"));
                match flag {
                    "|0" => {
                        assert!(decode[byte as usize].replace(cp).is_none(), "{name}: byte {byte:02X} mapped twice");
                        encode.push((cp, byte));
                    }
                    "|1" => encode.push((cp, byte)),
                    "|3" => {
                        decode[byte as usize].get_or_insert(cp);
                    }
                    _ => panic!("{name}: unknown precision flag in {line}"),
                }
            }
        }
    }
    encode.sort_unstable();
    for pair in encode.windows(2) {
        assert!(pair[0].0 != pair[1].0, "{name}: U+{:04X} encoded twice", pair[0].0);
    }
    let decode = decode.map(|d| d.unwrap_or_else(|| panic!("{name}: a byte with no mapping")));
    Page { ccsid, name, decode, encode }
}
