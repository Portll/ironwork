# Releasing ironwork

Walk these steps in order at every cut. `tools/release-check.mjs` in the cobolwork-web checkout
beside this one checks the steps a machine can:

```sh
node ../cobolwork-web/tools/release-check.mjs ironwork --before
node ../cobolwork-web/tools/release-check.mjs ironwork --after <version>
```

It prints PASS, FAIL or TODO for each step and exits 1 on any FAIL.

## Before the tag

1. **Claim the cut.** Read the cut task's owner and the active sessions in SPINE
   (`ironwork-roadmap`), then set the task active with your owner before any commit.
2. **CI.** `--before` passes only when main's last CI run is for main's head and every job passed.
   CI runs clippy from the latest stable Rust, so run that version locally too:
   `cargo +<stable> clippy --workspace --all-targets --locked -- -D warnings`.
3. **Notes.** Start from the commits `--before` lists since the previous release. Each feature and
   fix there is either named in the notes or left out on purpose.
4. **Version.** One commit bumps both workspaces: `version` under `[workspace.package]` in
   `Cargo.toml`, every internal `version = "x"` pin in `crates/*/Cargo.toml`, and
   `tls/Cargo.toml`. Then `cargo update -w` and `cargo update -w --manifest-path tls/Cargo.toml`,
   and `cargo check --locked --all-targets` and `cargo check --locked --manifest-path
   tls/Cargo.toml`, reading each exit code. `tls/` is a second workspace with its own lock.

## The tag and the registries

5. **Tag.** An annotated tag `v<version>` with the message `ironwork <version>`, on the commit
   whose CI passed. Pushing it runs `release.yml`: builds for five targets and the TLS builds,
   `SHA256SUMS`, provenance, the npm tarball, the GitHub release and PyPI.
6. **Notes.** The workflow's notes carry only the install paragraph. Add what the release contains
   with `gh release edit v<version> --notes-file <file>`.
7. **crates.io.** From the tagged commit, `cargo publish --workspace --dry-run --locked`, then
   `cargo publish --workspace --locked`; cargo orders the crates. A crate new since the last
   release publishes for the first time the same way.
8. **npm.** Download `portll-ironwork-<version>.tgz` from the release and check it against
   `SHA256SUMS`. `npm publish <tarball> --access public` needs the maintainer's browser 2FA. A
   later 409 "previously staged" means the publish is still processing.

## After

9. **The site.** In cobolwork-web, add the row to `public/ironwork/releases/` and move the latest
   pill, then bring the Overview, Features and Roadmap pages to the release. Commit to main: the
   deploy job refuses while a release fact disagrees with GitHub, npm, PyPI or crates.io, and
   publishes when none does.
10. **Check.** `--after <version>` passes: GitHub's latest release, npm's latest dist-tag, PyPI,
    every workspace crate on crates.io, the site's pages and their live deploy.
11. **SPINE.** Complete the cut task and its parent, with the tag, the registries and the site
    version in the result.
12. **This file.** When a step changes, change it here in the same cut.
