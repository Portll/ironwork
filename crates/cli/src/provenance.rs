//! `--provenance FILE`: what a compile read and decided, as an in-toto statement with the SLSA
//! Provenance v1 predicate (cobolwork `docs/spec/evidence.md` §10). The source and every COPY
//! member are resolved dependencies by digest, named relative to the directory that supplied them;
//! the option cards as written and the options in force are parameters. Unsigned: the pipeline
//! that signs it earns any SLSA level.

use std::fs::File;
use std::path::{Path, PathBuf};

use exec::digest::{hex, sha256_reader};
use exec::evidence::{canonical, fields, Value};

pub const BUILD_TYPE: &str = "https://github.com/Portll/ironwork/blob/main/docs/evidence.md#check-v1";
const LOCAL_BUILDER: &str = "https://github.com/Portll/ironwork/local";

fn digest(path: &Path) -> Option<String> {
    File::open(path).and_then(sha256_reader).ok().map(|(d, _)| hex(&d))
}

/// `path` relative to the first root holding it, with that root's index, or its file name.
fn locate(path: &Path, roots: &[PathBuf]) -> (String, Option<usize>) {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    for (i, root) in roots.iter().enumerate() {
        let root = std::path::absolute(root).unwrap_or_else(|_| root.clone());
        if let Ok(rest) = absolute.strip_prefix(&root) {
            return (rest.to_string_lossy().replace('\\', "/"), Some(i));
        }
    }
    (path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), None)
}

fn strings(items: impl IntoIterator<Item = String>) -> Value {
    Value::Arr(items.into_iter().map(Value::Str).collect())
}

pub struct Inputs<'a> {
    pub program: &'a str,
    pub sources: &'a [String],
    pub cards: &'a [String],
    pub flags: &'a [String],
    pub roots: &'a [PathBuf],
    pub compiled: &'a exec::Compiled,
    pub journal_tip: Option<(String, String)>,
}

pub fn statement(i: &Inputs<'_>) -> String {
    let program = Path::new(i.program);
    let name = program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let subject = Value::Arr(vec![Value::Obj(fields([("name", name.into()), ("digest", Value::Obj(fields([("sha256", digest(program).unwrap_or_default().into())])))]))]);

    let mut deps = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for s in std::iter::once(i.program.to_string()).chain(i.sources.iter().filter(|s| !s.is_empty()).cloned()) {
        if !seen.insert(s.clone()) {
            continue;
        }
        if s.starts_with('(') {
            deps.push(Value::Obj(fields([("name", s.trim_matches(|c| c == '(' || c == ')').to_string().into())])));
            continue;
        }
        let path = PathBuf::from(&s);
        let (rel, root) = locate(&path, i.roots);
        let mut d = fields([("uri", format!("file:{rel}").into()), ("name", rel.into())]);
        if let Some(sha) = digest(&path) {
            d.insert("digest".into(), Value::Obj(fields([("sha256", sha.into())])));
        }
        if let Some(r) = root {
            d.insert("annotations".into(), Value::Obj(fields([("library", Value::Int(r as i64))])));
        }
        deps.push(Value::Obj(d));
    }

    let o = &i.compiled.options;
    let in_force = Value::Obj(fields([
        ("arith", format!("{:?}", o.arith).into()),
        ("trunc", format!("{:?}", o.trunc).into()),
        ("numproc", format!("{:?}", o.numproc).into()),
        ("codepage", Value::Int(i64::from(o.codepage))),
        ("truncCheck", format!("{:?}", o.trunc_check).into()),
        ("fastsrt", o.fastsrt.into()),
        ("adv", o.adv.into()),
        ("compliance", o.compliance.name().into()),
        ("ssrange", i.compiled.ssrange.into()),
    ]));
    let libraries = strings(i.roots.iter().skip(1).map(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    let build_definition = Value::Obj(fields([
        ("buildType", BUILD_TYPE.into()),
        ("externalParameters", Value::Obj(fields([("optionCards", strings(i.cards.iter().cloned())), ("flags", strings(i.flags.iter().cloned())), ("libraries", libraries)]))),
        ("internalParameters", Value::Obj(fields([("optionsInForce", in_force)]))),
        ("resolvedDependencies", Value::Arr(deps)),
    ]));
    let builder = std::env::var("IRONWORK_BUILDER_ID").unwrap_or_else(|_| LOCAL_BUILDER.to_string());
    let mut builder_obj = fields([("id", builder.into()), ("version", Value::Obj(fields([("ironwork", env!("CARGO_PKG_VERSION").into())])))]);
    if let Some(sha) = std::env::current_exe().ok().and_then(|p| digest(&p)) {
        builder_obj.insert("builderDependencies".into(), Value::Arr(vec![Value::Obj(fields([("name", "ironwork".into()), ("digest", Value::Obj(fields([("sha256", sha.into())])))]))]));
    }
    let byproducts = match &i.journal_tip {
        Some((run, tip)) => Value::Arr(vec![Value::Obj(fields([("name", format!("evidence:run:{run}").into()), ("digest", Value::Obj(fields([("sha256", tip.clone().into())])))]))]),
        None => Value::Arr(Vec::new()),
    };
    let run_details = Value::Obj(fields([("builder", Value::Obj(builder_obj)), ("metadata", Value::Obj(Default::default())), ("byproducts", byproducts)]));
    let statement = Value::Obj(fields([
        ("_type", "https://in-toto.io/Statement/v1".into()),
        ("subject", subject),
        ("predicateType", "https://slsa.dev/provenance/v1".into()),
        ("predicate", Value::Obj(fields([("buildDefinition", build_definition), ("runDetails", run_details)]))),
    ]));
    format!("{}\n", canonical(&statement))
}
