//! `ironwork compile --native`: each load module written as a Rust crate that runs it from the
//! runtime alone (`rt::native`), then built with cargo into an executable beside the module.
//! The crate forbids `unsafe` and depends on the runtime only, at this compiler's own version.

use rt::lir::Program;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the generated crate finds the runtime: the published crate at this compiler's version, or
/// a checkout's `crates` directory.
pub enum Runtime {
    Published,
    Checkout(PathBuf),
}

/// The crate name a module's executable is built under: its name lower-cased, each character a crate
/// name cannot hold made `-`, after `cobol-`.
pub fn package(module: &str) -> String {
    let stem = module.strip_suffix(".iwm").unwrap_or(module);
    let name: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_lowercase() } else { '-' }).collect();
    format!("cobol-{name}")
}

/// The generated crate's manifest and main, the main holding each of `programs`' generated code.
pub fn crate_text(module: &str, runtime: &Runtime, programs: &[Program]) -> (String, String) {
    let dependency = match runtime {
        Runtime::Published => format!("version = \"={}\"", env!("CARGO_PKG_VERSION")),
        Runtime::Checkout(dir) => format!("path = \"{}\"", dir.join("rt").display().to_string().replace('\\', "/")),
    };
    let manifest = format!(
        "[package]\nname = \"{}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\nironwork-rt = {{ {dependency} }}\n\n[profile.release]\ncodegen-units = 1\nlto = \"thin\"\n\n[workspace]\n",
        package(module)
    );
    (manifest, crate::codegen::main_text(module, programs))
}

/// Writes `module`'s crate under `out` and builds it, the runtime's build shared by every module
/// built into `out`; the executable is copied beside the module.
pub fn build(out: &Path, module: &str, runtime: &Runtime) -> Result<PathBuf, String> {
    let stem = module.strip_suffix(".iwm").unwrap_or(module);
    let dir = out.join(".ironwork-native").join(stem);
    let bytes = fs::read(out.join(module)).map_err(|e| format!("{}: {e}", out.join(module).display()))?;
    let programs = rt::module::read(&bytes).map_err(|e| format!("{module}: {e}"))?.programs;
    let (manifest, main) = crate_text(module, runtime, &programs);
    let write = |path: &Path, bytes: &[u8]| fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()));
    fs::create_dir_all(dir.join("src")).map_err(|e| format!("{}: {e}", dir.display()))?;
    write(&dir.join("Cargo.toml"), manifest.as_bytes())?;
    write(&dir.join("src").join("main.rs"), main.as_bytes())?;
    write(&dir.join("src").join(module), &bytes)?;
    let target = out.join(".ironwork-native").join("target");
    let built = Command::new("cargo")
        .args(["build", "--release", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("cargo: {e}; --native needs a Rust toolchain"))?;
    if !built.success() {
        return Err(format!("cargo build of {} failed", dir.display()));
    }
    let executable = format!("{}{}", package(module), std::env::consts::EXE_SUFFIX);
    let to = out.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
    fs::copy(target.join("release").join(&executable), &to).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// codegen-runtime.md B5: the emitted Rust forbids `unsafe` and names the runtime alone.
    #[test]
    fn a_native_crate_forbids_unsafe_and_depends_on_the_runtime_alone() {
        for runtime in [Runtime::Published, Runtime::Checkout(PathBuf::from("/src/ironwork/crates"))] {
            let (manifest, main) = crate_text("PAYROLL.iwm", &runtime, &[]);
            assert!(main.starts_with("#![forbid(unsafe_code)]\n"));
            assert!(main.contains("ironwork_rt::native::main(\"PAYROLL.iwm\", include_bytes!(\"PAYROLL.iwm\"), &NATIVES)"));
            let dependencies: Vec<&str> = manifest.split("[dependencies]\n").nth(1).unwrap().split("\n\n").next().unwrap().lines().collect();
            assert_eq!(dependencies.len(), 1);
            assert!(dependencies[0].starts_with("ironwork-rt = "));
        }
        assert!(crate_text("A.iwm", &Runtime::Published, &[]).0.contains(&format!("version = \"={}\"", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn a_module_name_becomes_a_crate_name() {
        assert_eq!(package("PAYROLL.iwm"), "cobol-payroll");
        assert_eq!(package("A-B$1.iwm"), "cobol-a-b-1");
    }
}
