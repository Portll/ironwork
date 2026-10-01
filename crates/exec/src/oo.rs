//! Where a class definition is found. The run unit's objects and classes, and running them, are
//! `rt::oo`; compiling and checking a class is `compile::oo`.

pub use compile::oo::*;
pub use compile::oo::{ClassCode, MethodCode, Part};
pub use rt::oo::*;

use syntax::ast::Program;

pub(crate) fn refuse_to_run(program: &Program) -> Result<(), crate::Abend> {
    match program.oo.as_ref().and_then(|o| o.class()) {
        Some(c) => Err(crate::Abend { code: crate::abend::AbendCode::Ironwork, message: format!("{} is a class definition: run a program that uses it", c.name), pos: c.pos, file: None }),
        None => Ok(()),
    }
}

/// Where a COBOL class definition is found: among the programs already read, then in the program
/// libraries, as a member named with the class's simple name or its full name with periods as
/// underscores, with the path of the file it was read from. None means a Java class. See
/// [`numeric::assumptions::CLASS_SEARCH`].
pub(crate) fn find_class(library: &mut crate::unit::Library, external: &str) -> Result<Option<(Program, Option<String>)>, String> {
    if external == JAVA_LANG_OBJECT {
        return Ok(None);
    }
    if let Some(i) = library.programs.iter().position(|p| defined_class(p).as_deref() == Some(external)) {
        return Ok(Some((library.programs.remove(i), None)));
    }
    let member = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    let simple = external.rsplit('.').next().unwrap_or(external).to_owned();
    let mut names = vec![simple, external.replace('.', "_")];
    names.dedup();
    names.retain(|n| member(n));
    for dir in library.dirs.clone() {
        for name in &names {
            for variant in [name.clone(), name.to_ascii_lowercase(), name.to_ascii_uppercase()] {
                for ext in ["", ".cbl", ".CBL", ".cob", ".COB"] {
                    let path = dir.join(format!("{variant}{ext}"));
                    if !path.is_file() {
                        continue;
                    }
                    let text = std::fs::read(&path).map(|b| syntax::copy::decode(&b)).map_err(|e| format!("{}: {e}", path.display()))?;
                    let mut programs = syntax::parse_all_with(&text, &library.copy.with_program(&path)).map_err(|e| format!("class {external} does not compile: {}", e.place(&path.display().to_string())))?;
                    if defined_class(&programs[0]).as_deref() == Some(external) {
                        return Ok(Some((programs.remove(0), Some(path.display().to_string()))));
                    }
                }
            }
        }
    }
    Ok(None)
}
