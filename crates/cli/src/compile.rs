//! `ironwork compile`: each source's programs compiled, lowered and written as one load module
//! (docs/load-module.md), or every source's into one module under `--bundle`.

use exec::cics::{Cics, Datum};
use exec::lir::{Const, Operand, Program};
use exec::module::{DirectoryEntry, LoadedModule, SourceFile};
use rt::bms::Mapset;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

pub struct Request {
    pub sources: Vec<PathBuf>,
    pub out: PathBuf,
    pub bundle: Option<String>,
    pub libraries: Vec<PathBuf>,
    pub flags: Vec<String>,
    pub source_prefix: Option<String>,
}

/// A source's programs in ordinal order, with their directory entries, the files each one's debug
/// table names, the mapsets they name, and what each holds.
struct Lowered {
    programs: Vec<Program>,
    directory: Vec<DirectoryEntry>,
    files: Vec<Vec<Option<SourceFile>>>,
    mapsets: Vec<Mapset>,
    facts: Vec<Option<numeric::governs::Facts>>,
}

/// Every `.iwm` this request writes, as file name and the sources it holds.
fn outputs(r: &Request) -> Result<Vec<(String, Vec<usize>)>, String> {
    if let Some(name) = &r.bundle {
        let name = name.strip_suffix(".iwm").unwrap_or(name);
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return Err(format!("--bundle {name}: a module name, not a path"));
        }
        return Ok(vec![(format!("{name}.iwm"), (0..r.sources.len()).collect())]);
    }
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (k, source) in r.sources.iter().enumerate() {
        let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if stem.is_empty() {
            return Err(format!("{}: no file name to name its module", source.display()));
        }
        let name = format!("{stem}.iwm");
        if let Some((_, first)) = out.iter().find(|(n, _)| *n == name) {
            return Err(format!("{} and {} would both write {name}; compile them apart or give --bundle", r.sources[first[0]].display(), source.display()));
        }
        out.push((name, vec![k]));
    }
    Ok(out)
}

pub fn run(r: Request) -> ExitCode {
    if let Some(prefix) = &r.source_prefix
        && let Err(e) = portable(Path::new(prefix))
    {
        return crate::usage_error(&format!("--source-prefix {prefix}: {e}"));
    }
    let outputs = match outputs(&r) {
        Ok(o) => o,
        Err(e) => return crate::usage_error(&e),
    };
    let at = match exec::compile_time() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ironwork: {e}");
            return ExitCode::from(2);
        }
    };
    let mut status = 0u8;
    let bundled = if r.bundle.is_some() { bundled_programs(&r) } else { Vec::new() };
    let lowered: Vec<Option<Lowered>> = r
        .sources
        .iter()
        .map(|source| match lower_source(source, &r, at, &bundled) {
            Ok((l, code)) => {
                status = status.max(code);
                Some(l)
            }
            Err(code) => {
                status = status.max(code);
                None
            }
        })
        .collect();
    for (name, members) in outputs {
        let mut all = Lowered { programs: Vec::new(), directory: Vec::new(), files: Vec::new(), mapsets: Vec::new(), facts: Vec::new() };
        let Some(parts) = members.iter().map(|&k| lowered[k].as_ref()).collect::<Option<Vec<_>>>() else {
            eprintln!("ironwork: {name} not written");
            continue;
        };
        let mut mapsets: BTreeMap<String, Mapset> = BTreeMap::new();
        let mut differ = None;
        for part in parts {
            let base = all.programs.len() as u32;
            all.programs.extend(part.programs.iter().cloned());
            all.files.extend(part.files.iter().cloned());
            all.facts.extend(part.facts.iter().copied());
            all.directory.extend(part.directory.iter().map(|e| DirectoryEntry { parent: e.parent.map(|p| p + base), ..e.clone() }));
            for mapset in &part.mapsets {
                match mapsets.get(&mapset.name) {
                    Some(held) if held != mapset => differ = Some(mapset.name.clone()),
                    _ => {
                        mapsets.insert(mapset.name.clone(), mapset.clone());
                    }
                }
            }
        }
        if let Some(mapset) = differ {
            eprintln!("ironwork: {name}: two sources read different mapsets named {mapset}; compile them apart");
            status = status.max(12);
            continue;
        }
        all.mapsets = mapsets.into_values().collect();
        let module = LoadedModule { directory: all.directory, programs: all.programs, mapsets: all.mapsets, files: all.files, facts: all.facts };
        let bytes = match exec::module::write_module(&module) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("ironwork: {name}: the lowered programs make no valid module: {e}");
                status = status.max(16);
                continue;
            }
        };
        let path = r.out.join(&name);
        if let Err(e) = fs::create_dir_all(&r.out).and_then(|()| replace(&path, &bytes)) {
            eprintln!("ironwork: {}: {e}", path.display());
            status = status.max(16);
        }
    }
    ExitCode::from(status)
}

/// Writes beside the target and renames, so a reader never sees half a module.
fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let partial = path.with_extension(format!("iwm.{}.partial", std::process::id()));
    fs::write(&partial, bytes)?;
    fs::rename(&partial, path).inspect_err(|_| {
        let _ = fs::remove_file(&partial);
    })
}

/// Compiles and lowers every program of one source. Messages go to standard error; the error is
/// the return code that stops the module.
/// The PROGRAM-IDs of every source of a bundle, which a static CALL in any of them resolves to.
fn bundled_programs(r: &Request) -> Vec<String> {
    let compliance = numeric::Compliance::of(&r.flags);
    r.sources.iter().filter_map(|s| fs::read(s).ok()).flat_map(|bytes| syntax::program_ids(&syntax::copy::decode(&bytes), compliance)).collect()
}

fn lower_source(source: &Path, r: &Request, at: exec::lir::CompileTime, bundled: &[String]) -> Result<(Lowered, u8), u8> {
    let shown = source.display().to_string();
    let text = match fs::read(source) {
        Ok(bytes) if bytes.starts_with(&exec::module::MAGIC[..4]) => {
            eprintln!("ironwork: {shown} is a load module; compile takes a source");
            return Err(16);
        }
        Ok(bytes) => syntax::copy::decode(&bytes),
        Err(e) => {
            eprintln!("ironwork: {shown}: {e}");
            return Err(16);
        }
    };
    let own_directory = source.parent().map(Path::to_path_buf).unwrap_or_default();
    let dirs: Vec<PathBuf> = std::iter::once(own_directory).chain(r.libraries.iter().cloned()).collect();
    let libraries = syntax::copy::Libraries::new(dirs.clone()).with_program(source).with_flags(&r.flags);
    let parsed = match syntax::parse_all_with(&text, &libraries) {
        Ok(p) => p,
        Err(e) => return Err(crate::report(std::slice::from_ref(&e), &shown).max(12)),
    };
    let parsed: Vec<_> = parsed.into_iter().filter(|p| !p.is_prototype()).collect();
    let parents = parents(&parsed);
    let mut code = 0u8;
    let mut lowered = Lowered { programs: Vec::new(), directory: Vec::new(), files: Vec::new(), mapsets: Vec::new(), facts: Vec::new() };
    let mut read = None;
    let mut named = BTreeSet::new();
    for (mut ast, parent) in parsed.into_iter().zip(parents) {
        ast.linked.extend(bundled.iter().cloned());
        let id = ast.id.clone();
        let params: Vec<bool> = ast.using.iter().map(|p| p.by_value).collect();
        let returning = ast.returning.is_some();
        let external = ast.function.as_ref().map(|f| f.external.clone());
        let common = ast.common;
        let paths = ast.sources.clone();
        let files = read.get_or_insert_with(|| source_files(source, &paths, &dirs)).clone();
        let mut compiled = match exec::compile_at(ast, &r.flags, at) {
            Ok(c) => c,
            Err(messages) => {
                let rc = crate::report(&messages, &shown);
                if rc == 0 {
                    eprintln!("ironwork: {shown}: {id}: NOCOMPILE is a syntax check, with no module to write");
                }
                return Err(rc);
            }
        };
        code = code.max(crate::report(&compiled.diagnostics, &shown));
        compiled.program.sources = match debug_names(source, &paths, &dirs, r.source_prefix.as_deref()) {
            Ok(names) => names,
            Err(e) => {
                eprintln!("{shown}: {id}: {e}");
                return Err(12);
            }
        };
        let program = match exec::lower::lower(&compiled) {
            Ok(p) => p,
            Err(e) => return Err(crate::report(&[syntax::Error::from(e).in_files(&paths)], &shown).max(12)),
        };
        let entries = program.services.entries.iter().map(|e| (program.symbols.get(e.name as usize).cloned().unwrap_or_default(), e.paragraph)).collect();
        let id = program.symbols.get(program.id as usize).cloned().unwrap_or_default();
        mapsets_named(&program, &mut named);
        lowered.directory.push(DirectoryEntry { id, external, parent, common, entries, params, returning, dynamic: true });
        lowered.programs.push(program);
        lowered.files.push(files);
        lowered.facts.push(Some(exec::constructs::of(&compiled)));
    }
    for name in named {
        match syntax::bms::find_mapset(&libraries, &name) {
            None => {}
            Some(Ok(mapset)) => lowered.mapsets.push(mapset),
            Some(Err(e)) => {
                eprintln!("{shown}: mapset {name}: {}", e.message);
                return Err(12);
            }
        }
    }
    Ok((lowered, code))
}

/// The mapsets a SEND MAP or RECEIVE MAP of the program, or of a class's data or methods, names by
/// a literal: MAPSET, else MAP, upper-cased as the run reads them. One named by a data item is
/// found only when the module runs.
fn mapsets_named(program: &Program, out: &mut BTreeSet<String>) {
    for command in &program.services.cics {
        let (Cics::SendMap { map, mapset, .. } | Cics::ReceiveMap { map, mapset, .. }) = &command.command else { continue };
        if let Some(name) = mapset.as_ref().or(map.as_ref()).and_then(|d| literal(program, d)) {
            out.insert(name);
        }
    }
    if let Some(class) = &program.services.class {
        for part in class.factory.iter().chain(&class.object) {
            mapsets_named(&part.data, out);
        }
        for method in &class.methods {
            mapsets_named(&method.code, out);
        }
    }
}

fn literal(program: &Program, datum: &Datum) -> Option<String> {
    let text = match datum {
        Datum::Text(t) => program.symbols.get(*t as usize)?.trim().trim_matches(['\'', '"']).to_owned(),
        Datum::Value(Operand::Const(c)) => match program.consts.get(*c as usize)? {
            Const::Bytes(bytes) => program.options.options.code_page().decode(bytes).trim_end().to_owned(),
            _ => return None,
        },
        _ => return None,
    };
    (!text.is_empty()).then(|| text.to_ascii_uppercase())
}

/// Each program's containing program: the nearest before it that lists it among those it contains.
fn parents(programs: &[syntax::ast::Program]) -> Vec<Option<u32>> {
    (0..programs.len())
        .map(|k| (0..k).rev().find(|&j| programs[j].nested.iter().any(|n| n == &programs[k].id)).map(|j| j as u32))
        .collect()
}

/// The file each name of the debug table stands for, as a run of the source's journal records it
/// (load-module.md §9.2): the source, then each COPY member, by the library it was found in and its
/// digest. A member the compiler supplies has no file.
fn source_files(source: &Path, paths: &[String], dirs: &[PathBuf]) -> Vec<Option<SourceFile>> {
    let file = |path: &Path| {
        let (sha256, bytes) = crate::evidence::digest_bytes(path)?;
        let root = u32::try_from(crate::evidence::root_of(path, dirs).max(0)).unwrap_or_default();
        Some(SourceFile { root, path: crate::evidence::relative(path, dirs), sha256, bytes })
    };
    std::iter::once(file(source)).chain(paths.iter().skip(1).map(|p| if p.starts_with("(system member ") { None } else { file(Path::new(p)) })).collect()
}

/// The debug table's file names (load-module.md §9.1): the source's file name, behind
/// `--source-prefix`, then each COPY member relative to the library that supplied it.
fn debug_names(source: &Path, paths: &[String], dirs: &[PathBuf], prefix: Option<&str>) -> Result<Vec<String>, String> {
    let file = source.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let main = match prefix {
        Some(p) if !p.is_empty() => format!("{}/{file}", p.trim_end_matches('/')),
        _ => file,
    };
    let mut names = vec![main];
    for path in paths.iter().skip(1) {
        let member = Path::new(path);
        let supplied = dirs.iter().filter_map(|d| member.strip_prefix(d).ok().map(|rest| (d.components().count(), rest))).max_by_key(|(depth, _)| *depth);
        names.push(match supplied {
            Some((_, rest)) => portable(rest).map_err(|e| format!("COPY member {path}: {e}"))?,
            None if path.starts_with("(system member ") => path.clone(),
            None => return Err(format!("COPY member {path} is in none of the libraries")),
        });
    }
    Ok(names)
}

/// A relative path with `/` between its parts; a `..`, a root or a drive letter would put a path
/// into the module.
fn portable(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for c in path.components() {
        match c {
            Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => return Err("a name with a .. component".into()),
            Component::RootDir | Component::Prefix(_) => return Err("an absolute path, or one with a drive letter".into()),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_is_named_from_the_library_that_supplied_it() {
        let dirs = [PathBuf::new(), PathBuf::from("/build/copy"), PathBuf::from("/build/copy/sys")];
        let paths = ["".to_string(), "CUSTREC.cpy".into(), "/build/copy/LIB/X.cpy".into(), "/build/copy/sys/SQLCA.cpy".into(), "(system member DFHAID)".into()];
        let names = debug_names(Path::new("src/PAYROLL.cbl"), &paths, &dirs, None).unwrap();
        assert_eq!(names, ["PAYROLL.cbl", "CUSTREC.cpy", "LIB/X.cpy", "SQLCA.cpy", "(system member DFHAID)"]);
        let prefixed = debug_names(Path::new("/abs/src/PAYROLL.cbl"), &paths[..1], &dirs, Some("app/"));
        assert_eq!(prefixed.unwrap(), ["app/PAYROLL.cbl"]);
    }

    #[test]
    fn a_path_that_leaves_its_library_is_refused() {
        assert!(portable(Path::new("../X.cpy")).is_err());
        assert!(portable(Path::new("/X.cpy")).is_err());
        assert_eq!(portable(Path::new("./a/b.cpy")).unwrap(), "a/b.cpy");
        let dirs = [PathBuf::from("lib")];
        assert!(debug_names(Path::new("P.cbl"), &["".into(), "lib/../X.cpy".into()], &dirs, None).is_err());
    }
}
