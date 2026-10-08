# The hardened build

A build of ironwork for running programs that come from anywhere, with the stricter choice made
wherever ironwork offers one: its run limits on, no network, and nothing taken from the
environment unless a flag allows it. The same source builds both; the hardened build is a Cargo
feature and a profile.

    cargo build --profile hardened --features hardened --locked -p ironwork
    cargo build --profile hardened --features hardened --locked --manifest-path tls/Cargo.toml

The binary is `target/hardened/ironwork` (`tls/target/hardened/ironwork` for the TLS build), and
`ironwork --version` names it: `ironwork for COBOL 0.9.0 (hardened)`.

**Status:** built 2026-10-08 (ironwork-roadmap 30), on the operator's request for a version that
follows security best practice by default, in a non-permissive Rust environment.

## 1. What it changes at run time

| | Default build | Hardened build |
|---|---|---|
| `run`, `job` and `cics` time limit | none | 3600 seconds, unless `--time-limit` gives another |
| Their storage limit | none | 1 GiB of run-unit storage, unless `--storage-limit` gives another |
| `--sql-db`, which connects to PostgreSQL | allowed | refused unless `--allow-network` |
| `cics --serve`, which listens on a port | allowed | refused unless `--allow-network` |
| A DD named by `DD_NAME` in the environment | read | ignored unless `--allow-environment`; `--dd` gives DDs |

Each limit ends a run as [SECURITY.md](../SECURITY.md) describes. `--allow-network` and
`--allow-environment` are accepted by the default build too, where they change nothing, so a
script can name them under either build. A job step's DDs come from its JCL under both builds.

## 2. The build

The `hardened` profile is the release profile with:

| Setting | Why |
|---|---|
| `overflow-checks = true` | integer overflow in ironwork itself stops the run instead of wrapping |
| `panic = "abort"` | a panic ends the process at once; no unwinding through the runtime's state |
| `lto = "fat"`, `codegen-units = 1` | one optimised unit, the code the checks guard |
| `strip = "symbols"` | no symbol table in the shipped binary |

Rust already gives every build what C toolchains need flags for: memory safety, bounds-checked
indexing, position-independent executables, full RELRO and a non-executable stack on Linux.

## 3. The Rust environment, for every build

The workspace and the TLS build deny, in all their code and tests:

- `unsafe_code` (forbidden), `non_ascii_idents`, `meta_variable_misuse`, `unit_bindings`,
  `unused_import_braces` and `unused_lifetimes`;
- Clippy's `dbg_macro`, `todo`, `unimplemented`, `exit`, `mem_forget`, `float_cmp`,
  `lossy_float_literal`, `fn_to_numeric_cast_any`, `host_endian_bytes`, `infinite_loop`,
  `large_include_file`, `large_stack_arrays`, `suspicious_xor_used_as_pow`, `cfg_not_test`,
  `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`, `rc_mutex`, `rc_buffer`,
  `mutex_atomic`, `empty_drop`, `try_err` and `pathbuf_init_then_push`;
- and every Clippy warning, in CI (`-D warnings`).

Not denied, with the reason: `indexing_slicing`, `arithmetic_side_effects` and `as_conversions`
flag about 2,500, 2,500 and 1,500 places that Rust checks at run time already (bounds) or the
hardened profile checks (overflow); `unwrap_used` and `expect_used` flag about 140, each an
invariant of ironwork's own data.

## 4. Supply chain

ironwork's own workspace depends on no third-party crate. The TLS build adds rustls, ring,
webpki-roots and their dependencies. CI's `hardened build and supply chain` job runs
`cargo deny check` on both: no crate with a known advisory or yanked, licences from the list in
[deny.toml](../deny.toml) only, no wildcard version, and crates from crates.io alone. Releases
keep their checksums, CycloneDX bills of materials, attestations and reproducible Linux builds
([SECURITY.md](../SECURITY.md)).
