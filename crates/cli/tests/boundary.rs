//! The runtime (`rt`, `numeric`, `zarch`) reaches no compiler crate, and every dependency is a workspace crate.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// A runtime crate's directory, then the packages its `[dependencies]` and `[dev-dependencies]` may name.
const ALLOWED: &[(&str, &[&str], &[&str])] = &[
    ("zarch", &[], &[]),
    ("numeric", &["ironwork-zarch"], &[]),
    ("rt", &["ironwork-numeric", "ironwork-zarch"], &["ironwork-oracle"]),
];

const RUNTIME: &[&str] = &["ironwork-zarch", "ironwork-numeric", "ironwork-rt"];

/// What a runtime crate's tests may reach besides the runtime; what it reaches in turn is checked.
const TEST_SUPPORT: &[&str] = &["ironwork-oracle"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Normal,
    Dev,
    Build,
}

impl Kind {
    const ALL: [Self; 3] = [Self::Normal, Self::Dev, Self::Build];

    fn table(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Dev => "dev-dependencies",
            Self::Build => "build-dependencies",
        }
    }
}

#[derive(Clone, Debug)]
struct Dependency {
    kind: Kind,
    /// Which table of the manifest declared it, so dotted keys join the right entry.
    table: usize,
    key: String,
    package: String,
    path: Option<String>,
    workspace: bool,
}

impl Dependency {
    fn new(kind: Kind, table: usize, key: &str) -> Self {
        Self { kind, table, key: key.to_owned(), package: key.to_owned(), path: None, workspace: false }
    }

    fn set(&mut self, field: &str, value: &str) {
        match field {
            "package" => self.package = unquote(value).to_owned(),
            "path" => self.path = Some(unquote(value).to_owned()),
            "workspace" => self.workspace = value.trim() == "true",
            _ => {}
        }
    }

    /// Reads `{ package = "..", path = ".." }`; a bare version string sets nothing.
    fn set_inline(&mut self, value: &str) {
        let Some(body) = value.trim().strip_prefix('{').and_then(|v| v.strip_suffix('}')) else { return };
        for pair in split_top(body, ',') {
            if let [field, value] = split_top(pair, '=')[..] {
                self.set(unquote(field), value);
            }
        }
    }

    fn describe(&self) -> String {
        let path = self.path.as_deref().unwrap_or(if self.workspace { "inherited" } else { "none" });
        format!("[{}] {} (package {}, path {path})", self.kind.table(), self.key, self.package)
    }
}

/// Each character of `s` outside quotes, with its byte index and the bracket depth before it.
fn outside_quotes(s: &str) -> Vec<(usize, char, i32)> {
    let (mut quote, mut depth, mut out) = (None, 0, Vec::new());
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        out.push((i, c, depth));
        match c {
            '"' | '\'' => quote = Some(c),
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    out
}

/// `s` split at each `sep` outside quotes and brackets.
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c, depth) in outside_quotes(s) {
        if c == sep && depth == 0 {
            parts.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&s[start..]);
    parts
}

fn unquote(s: &str) -> &str {
    s.trim().trim_matches('"').trim_matches('\'')
}

/// The manifest's lines without comments, with a value that spans lines joined into one.
fn logical_lines(manifest: &str) -> Vec<String> {
    let (mut lines, mut pending, mut open) = (Vec::new(), String::new(), 0);
    for raw in manifest.lines() {
        let line = outside_quotes(raw).into_iter().find(|&(_, c, _)| c == '#').map_or(raw, |(i, _, _)| &raw[..i]);
        open += outside_quotes(line).iter().fold(0, |n, &(_, c, _)| match c {
            '[' | '{' => n + 1,
            ']' | '}' => n - 1,
            _ => n,
        });
        pending.push_str(line.trim());
        pending.push(' ');
        if open <= 0 {
            lines.push(pending.trim().to_owned());
            pending.clear();
            open = 0;
        }
    }
    lines.push(pending.trim().to_owned());
    lines
}

/// The dependency kind a header opens, and the name if it is one dependency's own table.
fn header_table(header: &str) -> Option<(Kind, Option<&str>)> {
    let inner = header.strip_prefix('[')?.trim_end().strip_suffix(']')?;
    let parts: Vec<&str> = split_top(inner, '.').into_iter().map(unquote).collect();
    let at = match parts.first() {
        Some(&"target") => 2,
        Some(&"workspace") => 1,
        _ => 0,
    };
    let kind = Kind::ALL.into_iter().find(|k| parts.get(at) == Some(&k.table()))?;
    match parts[at + 1..] {
        [] => Some((kind, None)),
        [name] => Some((kind, Some(name))),
        _ => None,
    }
}

fn dependencies(manifest: &str) -> Vec<Dependency> {
    let mut found: Vec<Dependency> = Vec::new();
    let (mut table, mut tables) = (None, 0);
    for line in logical_lines(manifest) {
        if line.starts_with("[[") {
            table = None;
        } else if line.starts_with('[') {
            tables += 1;
            table = header_table(&line).map(|(kind, name)| {
                (
                    kind,
                    name.map(|name| {
                        found.push(Dependency::new(kind, tables, name));
                        found.len() - 1
                    }),
                )
            });
        } else if let [key, value] = split_top(&line, '=')[..] {
            let key: Vec<&str> = split_top(key, '.').into_iter().map(unquote).collect();
            match (table, key.as_slice()) {
                (Some((_, Some(own))), [field]) => found[own].set(field, value),
                (Some((kind, None)), [name, rest @ ..]) => {
                    let joined = if rest.is_empty() {
                        None
                    } else {
                        found.iter().position(|d| d.table == tables && d.key == *name)
                    };
                    let at = joined.unwrap_or_else(|| {
                        found.push(Dependency::new(kind, tables, name));
                        found.len() - 1
                    });
                    match rest {
                        [] => found[at].set_inline(value),
                        [field] => found[at].set(field, value),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    found
}

fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in logical_lines(manifest) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package
            && let [key, value] = split_top(&line, '=')[..]
            && unquote(key) == "name"
        {
            return Some(unquote(value).to_owned());
        }
    }
    None
}

struct Crate {
    dir: String,
    package: String,
    deps: Vec<Dependency>,
}

struct Workspace {
    crates: Vec<Crate>,
    /// `[workspace.dependencies]` of the root manifest.
    inherited: Vec<Dependency>,
}

impl Workspace {
    fn read(root: &Path) -> Self {
        let text = |path: &Path| fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut members = Vec::new();
        for entry in fs::read_dir(root.join("crates")).expect("crates/") {
            let dir = entry.expect("crates/ entry").path();
            let manifest = dir.join("Cargo.toml");
            if manifest.is_file() {
                let name = dir.file_name().expect("a crate directory").to_string_lossy().into_owned();
                members.push((name, text(&manifest)));
            }
        }
        members.sort();
        let members: Vec<(&str, &str)> = members.iter().map(|(d, m)| (d.as_str(), m.as_str())).collect();
        Self::parse(&text(&root.join("Cargo.toml")), &members)
    }

    fn parse(root: &str, members: &[(&str, &str)]) -> Self {
        let crates = members
            .iter()
            .map(|(dir, manifest)| Crate {
                dir: (*dir).to_owned(),
                package: package_name(manifest).unwrap_or_default(),
                deps: dependencies(manifest),
            })
            .collect();
        Self { crates, inherited: dependencies(root) }
    }

    /// The workspace crate that `d`'s path, or the path it inherits, leads to from `base`.
    fn resolve(&self, base: &str, d: &Dependency) -> Option<&Crate> {
        let (base, path) = match (&d.path, d.workspace) {
            (Some(path), _) => (base, path.as_str()),
            (None, true) => ("", self.inherited.iter().find(|w| w.key == d.key)?.path.as_deref()?),
            (None, false) => return None,
        };
        let mut at: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    at.pop();
                }
                part => at.push(part),
            }
        }
        match at[..] {
            ["crates", dir] => self.crates.iter().find(|c| c.dir == dir),
            _ => None,
        }
    }

    fn direct<'w>(&'w self, c: &'w Crate, dev: bool) -> impl Iterator<Item = &'w Crate> {
        let base = format!("crates/{}", c.dir);
        c.deps.iter().filter(move |d| dev || d.kind != Kind::Dev).filter_map(move |d| self.resolve(&base, d))
    }

    /// Each crate a build of `start` or its tests compiles, with the direct dependency it comes through.
    fn reach<'w>(&'w self, start: &'w Crate) -> Vec<(&'w str, &'w str)> {
        let mut seen = BTreeSet::new();
        let mut reached = Vec::new();
        let mut stack: Vec<(&Crate, &str)> = self.direct(start, true).map(|c| (c, c.package.as_str())).collect();
        while let Some((c, via)) = stack.pop() {
            if seen.insert(c.package.as_str()) {
                reached.push((c.package.as_str(), via));
                stack.extend(self.direct(c, false).map(|next| (next, via)));
            }
        }
        reached
    }

    fn boundary(&self) -> Vec<String> {
        let mut breaches = Vec::new();
        for &(dir, normal, dev) in ALLOWED {
            let Some(c) = self.crates.iter().find(|c| c.dir == dir) else {
                breaches.push(format!("crates/{dir} is missing"));
                continue;
            };
            for d in &c.deps {
                let allowed: &[&str] = match d.kind {
                    Kind::Normal => normal,
                    Kind::Dev => dev,
                    Kind::Build => &[],
                };
                let target = self.resolve(&format!("crates/{dir}"), d).map(|t| t.package.as_str());
                if !target.is_some_and(|t| allowed.contains(&t)) {
                    breaches.push(format!("crates/{dir}/Cargo.toml: {}", d.describe()));
                }
            }
            for (reached, via) in self.reach(c) {
                if !RUNTIME.contains(&reached) && !TEST_SUPPORT.contains(&reached) {
                    breaches.push(format!("{} reaches {reached} through {via}", c.package));
                }
            }
        }
        breaches
    }

    fn outside(&self) -> Vec<String> {
        let root = self
            .inherited
            .iter()
            .filter(|d| self.resolve("", d).is_none())
            .map(|d| format!("Cargo.toml: {}", d.describe()));
        let members = self.crates.iter().flat_map(|c| {
            let base = format!("crates/{}", c.dir);
            c.deps
                .iter()
                .filter(move |d| self.resolve(&base, d).is_none())
                .map(move |d| format!("crates/{}/Cargo.toml: {}", c.dir, d.describe()))
        });
        root.chain(members).collect()
    }
}

fn this_workspace() -> Workspace {
    Workspace::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn the_runtime_reaches_no_compiler_crate() {
    let breaches = this_workspace().boundary();
    assert!(
        breaches.is_empty(),
        "the runtime depends on the compiler or on a crate outside its rule:\n{}",
        breaches.join("\n")
    );
}

#[test]
fn every_dependency_is_a_crate_of_this_workspace() {
    let outside = this_workspace().outside();
    assert!(
        outside.is_empty(),
        "a dependency that is not a path to a crate of this workspace:\n{}",
        outside.join("\n")
    );
}

const ROOT: &str = "[workspace]\nmembers = [\"crates/*\"]\n";

fn synthetic(root: &str, rt: &str, oracle: &str) -> Workspace {
    let rt = format!("[package]\nname = \"ironwork-rt\"\n{rt}");
    let oracle = format!("[package]\nname = \"ironwork-oracle\"\n{oracle}");
    Workspace::parse(
        root,
        &[
            (
                "cli",
                "[package]\nname = \"ironwork\"\n[dependencies]\nexec = { package = \"ironwork-exec\", path = \"../exec\" }\n",
            ),
            ("compile", "[package]\nname = \"ironwork-compile\"\n"),
            ("exec", "[package]\nname = \"ironwork-exec\"\n"),
            (
                "numeric",
                "[package]\nname = \"ironwork-numeric\"\n\n[dependencies]\nzarch = { package = \"ironwork-zarch\", path = \"../zarch\" }\n",
            ),
            ("oracle", &oracle),
            ("rt", &rt),
            ("syntax", "[package]\nname = \"ironwork-syntax\"\n"),
            ("zarch", "[package]\nname = \"ironwork-zarch\"\n"),
        ],
    )
}

#[test]
fn the_check_sees_every_form_a_manifest_can_name_a_dependency_in() {
    let breaches = |rt: &str| synthetic(ROOT, rt, "").boundary();
    let clean = [
        "",
        "[dependencies]\nnumeric = { package = \"ironwork-numeric\", path = \"../numeric\", version = \"0.1.1\" }\n",
        "[dependencies]\nzarch.package = \"ironwork-zarch\"\nzarch.path = \"../zarch\"\n",
        "[dependencies.numeric]\npackage = \"ironwork-numeric\"\npath = \"../numeric\"\n",
        "[dev-dependencies]\noracle = { package = \"ironwork-oracle\", path = \"../oracle\" }\n",
        "[dependencies]\n# syntax = { package = \"ironwork-syntax\", path = \"../syntax\" }\n",
    ];
    for rt in clean {
        assert_eq!(breaches(rt), Vec::<String>::new(), "{rt}");
    }
    let caught = [
        "[dependencies]\nparser = { package = \"ironwork-syntax\", path = \"../syntax\", version = \"0.1.1\" }\n",
        "[dependencies]\nparser = { package = \"ironwork-syntax\", version = \"0.1.1\" }\n",
        "[dependencies]\nnumeric = { package = \"ironwork-numeric\", path = \"../syntax\" }\n",
        "[dependencies]\nlower = { path = \"../compile\" }\n",
        "[dependencies]\nsyntax.path = \"../syntax\"\n",
        "[dependencies]\n\"syntax\" = { path = \"../syntax\" } # the parser\n",
        "[dependencies.parser]\npackage = \"ironwork-syntax\"\npath = \"../syntax\"\n",
        "[dependencies]\nsyntax = {\n    package = \"ironwork-syntax\",\n    path = \"../syntax\",\n}\n",
        "[dev-dependencies]\nexec = { package = \"ironwork-exec\", path = \"../exec\" }\n",
        "[build-dependencies]\nnumeric = { package = \"ironwork-numeric\", path = \"../numeric\" }\n",
        "[target.'cfg(unix)'.dependencies]\ndriver = { package = \"ironwork\", path = \"../cli\" }\n",
        "[target.'cfg(target_os = \"linux\")'.dev-dependencies.parser]\npackage = \"ironwork-syntax\"\npath = \"../syntax\"\n",
        "[dependencies]\nlibc = \"0.2\"\n",
        "[dependencies]\nnumeric = { package = \"ironwork-numeric\", path = \"../../elsewhere/numeric\" }\n",
    ];
    for rt in caught {
        assert!(!breaches(rt).is_empty(), "not caught:\n{rt}");
    }
}

#[test]
fn the_check_follows_test_support_and_inherited_dependencies() {
    let oracle = "[dev-dependencies]\noracle = { package = \"ironwork-oracle\", path = \"../oracle\" }\n";
    let parser = "[dependencies]\nsyntax = { package = \"ironwork-syntax\", path = \"../syntax\" }\n";
    assert_eq!(
        synthetic(ROOT, oracle, parser).boundary(),
        ["ironwork-rt reaches ironwork-syntax through ironwork-oracle"]
    );
    assert_eq!(synthetic(ROOT, "", parser).boundary(), Vec::<String>::new());

    let root = format!(
        "{ROOT}\n[workspace.dependencies]\nsyntax = {{ package = \"ironwork-syntax\", path = \"crates/syntax\" }}\nlibc = \"0.2\"\n"
    );
    let inherited = synthetic(&root, "[dependencies]\nsyntax.workspace = true\n", "");
    assert!(!inherited.boundary().is_empty());
    assert_eq!(inherited.outside(), ["Cargo.toml: [dependencies] libc (package libc, path none)"]);
    let root = format!(
        "{ROOT}\n[workspace.dependencies]\nnumeric = {{ package = \"ironwork-numeric\", path = \"crates/numeric\" }}\n"
    );
    let numeric = synthetic(&root, "[dependencies]\nnumeric = { workspace = true }\n", "");
    assert!(numeric.boundary().is_empty() && numeric.outside().is_empty());

    let registry = synthetic(ROOT, "", "[dependencies]\nrand = { version = \"0.8\" }\n").outside();
    assert_eq!(registry, ["crates/oracle/Cargo.toml: [dependencies] rand (package rand, path none)"]);
}
