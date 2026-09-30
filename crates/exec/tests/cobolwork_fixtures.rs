//! The fixtures cobolwork shares (fixtures/cobolwork): each side reads the same maps and statements,
//! and a drift test holds ironwork's copies equal to cobolwork's.

use std::path::{Path, PathBuf};
use syntax::ast::Ref;
use syntax::sql::{Cursor, HostVar, Statement};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cobolwork")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace('\r', "")
}

const MAPSETS: [&str; 2] = ["COSGN00", "COCRDSL"];

/// Level, name, offset, size and OCCURS of every item a program gets from `COPY member`, with the
/// member read from a directory holding only `file`.
fn copied_layout(member: &str, file: &str) -> Vec<(u8, Option<String>, u32, u32, u32)> {
    let dir = std::env::temp_dir().join(format!("ironwork-cobolwork-{}-{file}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), read(&fixtures().join("bms").join(file))).unwrap();
    let program = format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n           COPY {member}.\n       PROCEDURE DIVISION.\n           GOBACK.\n"
    );
    let parsed = syntax::parse_with(&program, &syntax::copy::Libraries::new(vec![dir.clone()])).unwrap_or_else(|e| panic!("{file}: {e}"));
    let compiled = ironwork_exec::compile(parsed, &[]).unwrap_or_else(|e| panic!("{file}: {e:?}"));
    std::fs::remove_dir_all(dir).unwrap();
    compiled.layout.items.iter().map(|i| (i.level, i.name.clone(), i.offset, i.size, i.occurs)).collect()
}

/// CardDemo ships the copybooks CICS generated from its maps, so the symbolic map ironwork builds
/// from each map can be laid out beside the one CICS built, item by item.
#[test]
fn symbolic_maps_lay_out_as_the_copybooks_cics_generated() {
    for member in MAPSETS {
        let generated = copied_layout(member, &format!("{member}.bms"));
        assert!(generated.len() > 50, "{member}: {generated:?}");
        assert_eq!(generated, copied_layout(member, &format!("{member}.cpy")), "{member}");
    }
}

fn host_name(r: &Ref) -> String {
    r.qualifiers.iter().rev().chain(std::iter::once(&r.name)).cloned().collect::<Vec<_>>().join(".")
}

fn names(vars: &[HostVar]) -> Vec<String> {
    vars.iter().flat_map(|h| std::iter::once(&h.var).chain(&h.indicator)).map(host_name).collect()
}

/// The host variables a statement reads and those it writes, as ironwork's typed statement has them.
fn directions(statement: &Statement) -> (Vec<String>, Vec<String>) {
    match statement {
        Statement::Query { inputs, into, .. } => (names(inputs), names(into)),
        Statement::Change { inputs, .. } | Statement::DeclareCursor(Cursor { inputs, .. }) => (names(inputs), Vec::new()),
        Statement::Fetch { into, .. } => (Vec::new(), names(into)),
        Statement::Open { .. } | Statement::Close { .. } => (Vec::new(), Vec::new()),
        other => panic!("the shared table holds only statements ironwork runs: {other:?}"),
    }
}

#[test]
fn host_variables_are_read_and_written_as_the_shared_table_says() {
    let table = read(&fixtures().join("sql/host-variables.tsv"));
    let list = |cell: &str| if cell == "-" { Vec::new() } else { cell.split(' ').map(String::from).collect() };
    let rows: Vec<Vec<&str>> = table.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.split('\t').collect()).collect();
    assert!(rows.len() >= 20);
    for row in rows {
        let [statement, read, written] = row[..] else { panic!("a row has three cells: {row:?}") };
        let typed = syntax::sql::parse(statement, syntax::Pos::default());
        assert_eq!(directions(&typed), (list(read), list(written)), "{statement}");
    }
}

/// Each shared region definition, where cobolwork keeps it and the name ironwork's copy has.
const CSDS: [(&str, &str); 5] = [
    ("entry/REGION.csd", "entry.csd"),
    ("intrdr/declared/REGION.CSD", "intrdr-declared.csd"),
    ("outbound/REGION.csd", "outbound.csd"),
    ("priv/REGION.csd", "priv.csd"),
    ("web-csd/REGION.csd", "web-csd.csd"),
];

/// What cobolwork's parseCsd reads from each: transactions with their programs, and queues with
/// their type and DD.
#[test]
fn region_definitions_read_as_cobolwork_reads_them() {
    type Expected = (Vec<(&'static str, &'static str)>, Vec<(&'static str, &'static str, Option<&'static str>)>);
    let expected: [(&str, Expected); 5] = [
        ("entry.csd", (vec![("INQ1", "INQUIRY"), ("MNU1", "MENU")], vec![])),
        ("intrdr-declared.csd", (vec![("JSUB", "JOBSUB")], vec![("JOBS", "EXTRA", Some("INREADER")), ("LOGQ", "EXTRA", Some("APPLOG"))])),
        ("outbound.csd", (vec![], vec![("RPTQ", "EXTRA", Some("RPTOUT")), ("SCRQ", "INTRA", None)])),
        ("priv.csd", (vec![("CECI", "DFHECIP"), ("PAY1", "PAYADM"), ("PAY2", "PAYRPT"), ("PAY3", "PAYADM2")], vec![])),
        ("web-csd.csd", (vec![], vec![])),
    ];
    for (file, (transactions, queues)) in expected {
        let csd = syntax::csd::parse(&read(&fixtures().join("csd").join(file))).unwrap_or_else(|e| panic!("{file}: {e}"));
        let got: Vec<(&str, &str)> = csd.transactions.iter().map(|(t, d)| (t.as_str(), d.program.as_deref().unwrap_or("-"))).collect();
        assert_eq!(got, transactions, "{file}");
        let got: Vec<(&str, &str, Option<&str>)> = csd.tdqueues.iter().map(|(q, d)| (q.as_str(), d.kind.as_deref().unwrap_or("-"), csd.dd_of_queue(q))).collect();
        assert_eq!(got, queues, "{file}");
    }
}

/// Run with IRONWORK_COBOLWORK_DIR naming a cobolwork checkout; CI does.
#[test]
fn the_vendored_fixtures_are_cobolworks() {
    let Ok(dir) = std::env::var("IRONWORK_COBOLWORK_DIR") else { return };
    let theirs = Path::new(&dir).join("test/fixtures");
    for sub in ["bms", "sql"] {
        let files = |root: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(root.join(sub)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect();
            names.sort();
            names
        };
        let ours = files(&fixtures());
        assert_eq!(ours, files(&theirs), "fixtures/cobolwork/{sub} holds other files than cobolwork's: run tools/sync-cobolwork-fixtures.sh");
        for name in ours {
            let differs = read(&fixtures().join(sub).join(&name)) != read(&theirs.join(sub).join(&name));
            assert!(!differs, "fixtures/cobolwork/{sub}/{name} differs from cobolwork's: run tools/sync-cobolwork-fixtures.sh");
        }
    }
    let mut ours: Vec<String> = std::fs::read_dir(fixtures().join("csd")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    ours.sort();
    assert_eq!(ours, CSDS.map(|(_, name)| name), "fixtures/cobolwork/csd holds other files than CSDS names");
    for (path, name) in CSDS {
        let differs = read(&fixtures().join("csd").join(name)) != read(&theirs.join(path));
        assert!(!differs, "fixtures/cobolwork/csd/{name} differs from cobolwork's {path}: run tools/sync-cobolwork-fixtures.sh");
    }
}
