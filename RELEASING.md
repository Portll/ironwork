# Releasing ironwork

Walk these steps in order at every cut. `tools/release-check.mjs` in the cobolwork-web checkout
beside this one checks the steps a machine can:

```sh
node ../cobolwork-web/tools/release-check.mjs ironwork --before
node ../cobolwork-web/tools/release-check.mjs ironwork --after <version>
```

It prints PASS, FAIL or TODO for each step and exits 1 on any FAIL.

## Before the tag

1. **Claim the cut.** In SPINE (`ironwork-roadmap`), create a task of your own under the cut task,
   set it active, and `claim_files ["release:ironwork"]` on it before any commit. A second
   session's claim is refused and names the task that holds the cut. `set_status active` refuses
   nothing, so activating the cut task is not a claim.
2. **CI.** `--before` passes only when main's last CI run is for main's head and every job passed.
   CI runs clippy from the latest stable Rust, so run that version locally too:
   `cargo +<stable> clippy --workspace --all-targets --locked -- -D warnings`.
   Bump the cobolwork `ref` in `.github/workflows/ci.yml` deliberately and only to a cobolwork commit whose shared tables ironwork's drift tests pass against.
   Refresh what CI cannot measure in `docs/conformance/` with a release build of main's head, and
   commit it before the version commit: `tools/census.py <binary> <corpus> 3000 1 --json
   docs/conformance/census.json`, `cargo run --release -p ironwork-oracle -- hercules <dir> >
   docs/conformance/hercules.txt` (it exits 1 while cases disagree), `differential.json` from a
   run of `differential.yml` on main's head (`gh run download <run> -p 'campaign-*' -D shards`, then
   `tools/differential-campaign.py total shards/*/campaign.json`), and `nist.tsv` from CI's
   `conformance` artifact when a program's class has changed. Read the report CI writes: the release
   attaches it as `conformance-ironwork-<version>.md`.
3. **Notes.** Start from the commits `--before` lists since the previous release. Each feature and
   fix there is either named in the notes or left out on purpose. Commit the notes as
   `docs/releases/<version>.md` before the tag. They open with a `## Summary` section, which the
   site renders as the release's row: the first paragraph is the benefit, each line opening with a
   hyphen a sub-item, a paragraph opening `**Limit:**` the limit. A change to anything
   [STABILITY.md](STABILITY.md) lists is named there.
4. **Version.** One commit bumps both workspaces: `version` under `[workspace.package]` in
   `Cargo.toml`, every internal `version = "=x"` pin in `crates/*/Cargo.toml` (exact, so a
   release builds only with its own crates), and
   `tls/Cargo.toml`. Then `cargo update -w` and `cargo update -w --manifest-path tls/Cargo.toml`,
   and `cargo check --locked --all-targets` and `cargo check --locked --manifest-path
   tls/Cargo.toml`, reading each exit code. `tls/` is a second workspace with its own lock. The
   commit's subject is `chore: release ironwork <version>`.

## The tag and the registries

5. **Tag.** An annotated tag `v<version>` with the message `ironwork <version>`, on the commit
   whose CI passed. Pushing the tag runs `release.yml`.
   Its `check` job fails the run unless the tag is `v` plus the version at the tagged commit, that
   commit holds `docs/releases/<version>.md`, the commit is an ancestor of `origin/main`, and
   `ci.yml` has a successful run for that commit; every
   other job then skips. It also fixes the stable Rust version of the moment, which every build
   uses and the release notes name. The builds run next, with provenance, beside `semver`, which
   reports what `cargo semver-checks` finds changed in a published crate's API and gates nothing,
   since the crates' Rust API carries no promise (STABILITY.md), and `sbom`, which writes each
   build's CycloneDX bill of materials. Nothing publishes until the builds and `sbom` pass. Publishing is one job per registry, in order: GitHub release, npm, crates.io, PyPI. Each
   job needs every build and the job before it. The GitHub release job has no environment and
   runs when the builds pass; each registry job then waits in the run's "Review deployments"
   until the operator approves it.
   If a job fails, re-run that job from the run's page; never move or delete a release tag, since
   the tag ruleset forbids it. Until the npm trusted publisher is set (`npm trust github
   @portll/ironwork --repo Portll/ironwork --file release.yml --env npm --allow-publish`), the npm job
   fails and the jobs after it wait. To check the workflow without a release, run `gh workflow run
   release.yml -R Portll/ironwork --ref main -f dry_run=true`: the checks and builds run and every
   publish job is skipped.
6. **GitHub release.** The release job creates the GitHub release with `SHA256SUMS`, provenance,
   the npm tarball, the two bills of materials, each attested for its build's archives, and
   `conformance-ironwork-<version>.md`, which the `conformance` job writes from the tag. Its
   notes are `docs/releases/<version>.md` from the tagged commit, which step 3 committed, followed
   by an `## Install` section the job writes.
7. **crates.io.** The crates.io job publishes with `cargo publish --workspace --locked` after
   `rust-lang/crates-io-auth-action` trades the run's identity for a short-lived token; cargo
   orders the crates, and a crate new since the last release publishes the same way. Trusted
   publishing is set for each crate, so a crate that is new needs its trusted publisher added
   first.
8. **npm.** The npm job publishes `portll-ironwork-<version>.tgz`, the tarball the build packed and
   the release carries, with `npm publish --access public` through trusted publishing: no token,
   no 2FA prompt. A 409 "previously staged" means the publish is still processing. The tarball is
   named by a path that starts with `./`: npm reads `npm/<file>.tgz` as a GitHub repository.
   A re-run uses `release.yml` as the tag holds it, so a fault in a publish step can't be fixed for
   that tag; fix it on main and cut the next patch release, as 0.4.1 followed 0.4.0.

## After

9. **The site.** In cobolwork-web, `node tools/build-site.mjs --releases` renders the release's row
   and the latest pill from the GitHub release. Bring the Overview and Features pages to the
   release, and the Roadmap through `data/roadmap/ironwork.json` and `node tools/build-site.mjs`.
   Commit to main: the deploy job refuses while a rendered section differs from its source or a
   release fact disagrees with GitHub, npm, PyPI or crates.io, and publishes when none does.
10. **Check.** `--after <version>` passes: GitHub's latest release, npm's latest dist-tag, PyPI,
    every workspace crate on crates.io, the site's pages and their live deploy.
11. **SPINE.** Complete the cut task and its parent, with the tag, the registries and the site
    version in the result.
12. **This file.** When a step changes, change it here in the same cut.

## Withdrawing a release

A release tag cannot be moved or deleted, and a registry keeps a version once published, so a bad
release is withdrawn where it is offered and fixed by the next patch release, as 0.4.1 followed
0.4.0. In order:

1. **Say so.** Open an issue naming the version, what is wrong and the version to use, and add a
   line opening `**Withdrawn:**` to the top of the release's notes with `gh release edit
   v<version> --notes-file <file>`. For a vulnerability, follow [SECURITY.md](SECURITY.md) and
   publish the advisory with the fix.
2. **GitHub.** Mark the previous good release latest: `gh release edit v<previous> --latest`.
3. **npm.** `npm dist-tag add @portll/ironwork@<previous> latest`, then `npm deprecate
   @portll/ironwork@<version> "<reason>; use <previous>"`. Do not unpublish: npm never lets the
   version be published again, and anyone who pinned it breaks.
4. **crates.io.** `cargo yank --version <version> <crate>` for each workspace crate, `ironwork`
   first. A yanked version is not chosen for a new install or lock, and a lock that holds it keeps
   building. `cargo yank --undo` reverses it.
5. **PyPI.** Yank the release on its page at pypi.org (Manage, Options, Yank), giving the reason.
   pip then installs it only when asked for that version exactly.
6. **The fix.** Fix main, cut `<version>` plus one patch with the steps above, and once it is
   published undo steps 2 and 3 for the new version: it becomes GitHub's latest by its own release,
   and `latest` on npm by its publish.
7. **SPINE.** Record the withdrawal and the fix on the cut task.
