# What the repository adds

Mirror of nv-rs `docs/fork/PRACTICES.md`, for a project that is a clean-room reimplementation instead of a fork.

| Practice | Where | Notes |
| --- | --- | --- |
| One required check, `Pipeline Complete` | `.github/workflows/pipeline.yml`, `.github/rulesets/main.json` | One job; the checks are steps (`task check`), because a private repository bills each job rounded up to a minute. Documentation-only changes skip the Rust steps (a step decides, not a job, for the same reason). The org's reusable hygiene, PR-title and quarantine workflows cannot be steps, so they are jobs the last step gates on |
| Cancel superseded runs | `concurrency` in `pipeline.yml` | A new commit cancels the older run of a pull request; a title edit has its own group; `main` is never cancelled; no label-event triggers |
| Actions pinned by SHA | every workflow | The org's reusable workflows at github-workflows v0.50.3 (`8e402cc`), whose actions are SHA-pinned; Renovate proposes bumps |
| Branch protection | `.github/rulesets/`, `scripts/apply-ruleset.sh`, `scripts/setup-repo.sh` | Pull requests only, squash, linear history, no force push, `Pipeline Complete` required; `v*` tags immutable. Prepared here, applied by the owner (`--audit` first) |
| Format, clippy, tests, coverage | `task fmt`, `clippy`, `test` | clippy pedantic with warnings denied; 80% line coverage gate (cargo-llvm-cov) |
| Licences, bans, sources, advisories | `deny.toml`, `task deny`, `.github/workflows/security.yml` | permissive, MPL-2.0 and GPL-3.0 dependencies allowed (docs/LICENSING.md); the fuzz workspace is checked too; RustSec daily |
| Secret scanning | `.gitleaks.toml`, `task secrets` | Every commit |
| Workflow lint and security | `actionlint`, `zizmor` in `task lint:actions` | |
| Fuzzing | `fuzz/`, `.github/workflows/fuzz.yml` | Own workspace; one empty-but-compiling harness per future parser, type-checked on every pull request, run weekly; needs `cargo +nightly` |
| Releases | `release-please-config.json`, `.github/workflows/release.yml` | `release-type: simple` with `extra-files` for the version in `Cargo.toml` and `Cargo.lock`. The `rust` type would version each of the three crates separately (a virtual workspace needs the cargo-workspace plugin) and produce several tags; this repo has one product and one version. Nothing is published (no crates.io, `publish = false`) |
| Dependency updates | `renovate.json` | Extends the org preset; 7-day minimum release age applies on this machine too |
| Clean-room guards | `docs/clean-room.md`, `provenance/`, `scripts/cleanroom_guards.py`, `task guards` | SPDX headers, provenance references, the game-data guard, `.gitignore`. Not in nv-rs: the extra duty of a clean-room project |
| Pull request template | `.github/pull_request_template.md` | Provenance, no game data, clean-room statement |
| Local setup | `mise.toml`, `Taskfile.yml` | `mise install`, then `task check` |

Not adopted on purpose: Dependabot (Renovate is the estate's), GitHub dependency-review and CodeQL (they need Advanced
Security on a private repository; revisit if the repository goes public), Windows builds (nothing here is
Windows-only), nv-rs's sync and fork machinery (this is not a fork).

Provenance of the scripts: `scripts/setup-repo.sh` and `scripts/apply-ruleset.sh` are adapted from the owner's own
scripts in nv-rs, a GPL-3.0-only repository; the owner decides whether MPL-2.0 headers on them are right.
