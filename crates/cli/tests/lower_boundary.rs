//! Code generation (`crates/exec/src/lower`, its tests aside) names nothing that is exec's own: each
//! `crate::` path in it is `crate::lower` or a name exec's lib.rs re-exports from compile or rt, and
//! no `super::` leaves lower/.

use std::fs;
use std::path::{Path, PathBuf};

/// The names exec's lib.rs brings in from compile or rt with a `use`, as in `pub use rt::cics;`,
/// `use compile::sort;` and `pub(crate) use compile::{procedure, section_end};`.
fn reexports(lib: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in lib.lines().map(str::trim) {
        let Some(rest) = ["pub use ", "pub(crate) use ", "use "].iter().find_map(|p| line.strip_prefix(p)) else { continue };
        let Some(path) = rest.strip_prefix("rt::").or_else(|| rest.strip_prefix("compile::")) else { continue };
        let path = path.trim_end_matches(';');
        let items: Vec<&str> = match path.split_once('{') {
            Some((_, group)) => group.trim_end_matches('}').split(',').collect(),
            None => vec![path],
        };
        for item in items.into_iter().map(str::trim).filter(|i| !i.is_empty()) {
            let name = item.rsplit_once(" as ").map_or_else(|| item.rsplit("::").next().unwrap_or(item), |(_, alias)| alias);
            names.push(name.trim().to_owned());
        }
    }
    names
}

fn ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The first name of each path a `use` group starting at `group`, just past its `{`, lists.
fn group_heads(group: &str) -> Vec<&str> {
    let (mut depth, mut start, mut heads) = (0, 0, Vec::new());
    for (i, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => {
                heads.push(&group[start..i]);
                break;
            }
            '}' => depth -= 1,
            ',' if depth == 0 => {
                heads.push(&group[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    heads.into_iter().map(|h| h.trim().split(|c: char| !ident(c)).next().unwrap_or("")).filter(|h| !h.is_empty()).collect()
}

/// Each `crate::` path in `source` whose first name is neither `lower` nor in `allowed`, and each
/// `super::` chain longer than `depth`, the module's depth below lower/, by line.
fn breaches(source: &str, depth: usize, allowed: &[String]) -> Vec<(usize, String)> {
    let code: String = source.lines().map(|l| l.split_once("//").map_or(l, |(code, _)| code)).collect::<Vec<_>>().join("\n");
    let line = |at: usize| code[..at].matches('\n').count() + 1;
    let starts = |pattern: &'static str| code.match_indices(pattern).map(|(at, _)| at).filter(|&at| !code[..at].ends_with(|c: char| ident(c) || c == ':'));
    let mut found = Vec::new();
    for at in starts("crate::") {
        let rest = &code[at + "crate::".len()..];
        let heads = match rest.strip_prefix('{') {
            Some(group) => group_heads(group),
            None => vec![rest.split(|c: char| !ident(c)).next().unwrap_or("")],
        };
        for head in heads {
            if head != "lower" && !allowed.iter().any(|a| a == head) {
                found.push((line(at), format!("crate::{head}")));
            }
        }
    }
    for at in starts("super::") {
        let supers = code[at..].split("super::").skip(1).take_while(|s| s.is_empty()).count() + 1;
        if supers > depth {
            found.push((line(at), "super::".repeat(supers)));
        }
    }
    found
}

/// Each source file of lower/ but its tests, with its module's depth below lower/.
fn sources(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, usize)>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).map(|e| e.expect("a lower/ entry").path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().expect("a file name").to_string_lossy().into_owned();
        if name == "tests" || name == "tests.rs" {
            continue;
        }
        if path.is_dir() {
            sources(&path, depth + 1, out);
        } else if let Some(stem) = name.strip_suffix(".rs") {
            out.push((path, if stem == "mod" { depth } else { depth + 1 }));
        }
    }
}

#[test]
fn code_generation_names_nothing_of_exec_s_own() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = root.join("crates/exec/src/lib.rs");
    let allowed = reexports(&fs::read_to_string(&lib).unwrap_or_else(|e| panic!("{}: {e}", lib.display())));
    assert!(["layout", "Compiled", "procedure"].iter().all(|n| allowed.iter().any(|a| a == n)), "exec's lib.rs re-exports read as {allowed:?}");
    let mut files = Vec::new();
    sources(&root.join("crates/exec/src/lower"), 0, &mut files);
    assert!(files.len() > 10, "lower/ read as {files:?}");
    let mut found = Vec::new();
    for (path, depth) in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let shown = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
        found.extend(breaches(&source, depth, &allowed).into_iter().map(|(line, what)| format!("{shown}:{line}: {what}")));
    }
    assert!(found.is_empty(), "code generation names what is exec's own; take it from compile or rt:\n{}", found.join("\n"));
}

#[test]
fn the_check_sees_each_way_a_path_can_leave_lower() {
    let lib = "pub use compile::layout;\npub use rt::lir as ir;\npub(crate) use compile::{procedure, Compiled};\nuse compile::sort;\npub mod machine;\npub use machine::{Abend, Ending};\n";
    let allowed = reexports(lib);
    assert_eq!(allowed, ["layout", "ir", "procedure", "Compiled", "sort"]);
    let clean = [
        "use crate::layout::Resolved;\n",
        "use crate::lower::Lower;\n",
        "let p = crate::procedure(program, name);\n",
        "use crate::{Compiled, layout::Layout};\n",
        "use super::{Lower, R};\n",
        "use compile::oo::bodies;\n",
        "use rt::sql::fingerprint; // crate::machine::sql once\n",
        "pub(super) fn f() {}\n",
    ];
    for source in clean {
        assert_eq!(breaches(source, 1, &allowed), Vec::new(), "{source}");
    }
    let caught = [
        ("use crate::machine::cics_bind;\n", 1),
        ("for body in crate::oo::bodies(s) {}\n", 1),
        ("use crate::{layout::Resolved, sql::HostType};\n", 1),
        ("use crate::{\n    layout,\n    machine::{self, Machine},\n};\n", 1),
        ("let unit: &crate::unit::RunUnit = u;\n", 1),
        ("use crate::Execute;\n", 1),
        ("use super::super::vm;\n", 1),
        ("use super::flow::Ctx;\n", 0),
    ];
    for (source, depth) in caught {
        assert_eq!(breaches(source, depth, &allowed).len(), 1, "not caught once:\n{source}");
    }
    assert_eq!(breaches("\n\nuse crate::{\n    layout,\n    unit::RunUnit,\n};\n", 1, &allowed), [(3, "crate::unit".to_owned())]);
}
