# Contributing

This repository is private and owned by Joris Jonkers. Contributions happen by invitation.

## Before you start

Read [docs/clean-room.md](docs/clean-room.md). It decides which inputs you may use: never the unlicensed 2016
generator C# source (see docs/clean-room.md), decompiled or leaked code, or anything under an unknown licence. If you have seen such material,
say so before working on that area.

## Setup

```sh
mise install        # task, actionlint, gitleaks, cargo-deny, zizmor, cargo-llvm-cov
task check          # everything the pipeline runs
```

The Rust toolchain is pinned in `rust-toolchain.toml`. On NixOS wrap cargo in
`nix shell nixpkgs#gcc nixpkgs#lld -c`. Do not run the game, Wine, Proton or MO2 for any change here, and never put
game data in the repository, a pull request or an issue.

## Rules for a change

- Small pull requests with Conventional Commit titles (`feat:`, `fix:`, `docs:`, `ci:`, `chore:`); the title becomes the
  squash commit and release-please reads it. Do not hand-edit `CHANGELOG.md`.
- Every source file starts with `SPDX-License-Identifier: GPL-3.0-only`.
- Every format fact the code relies on is a row, one file per row in `provenance/rows/<id>.toml` whose `note` is its
  log entry, and referenced from code with `// source: <id>`. `provenance/sources.toml` and `provenance/LOG.md` are
  frozen history: a pull request does not add a line to them (the guard says where the entry belongs).
- Shared pages that every change used to edit are folders of files: a request option's note is a new file in
  `docs/contracts/options/`, a provenance row is a new file in `provenance/rows/`, a decision is a new file in
  `docs/decisions/`. Do not append to a shared log, table or page that other open pull requests also append to.
- Tests use synthetic fixtures from `lodwright-testkit`; nothing binary is committed (`docs/TESTING.md`).
- `unsafe` is forbidden; clippy pedantic and `-D warnings` apply.
- Fill in the pull request template; the pipeline's `Pipeline Complete` check must be green to merge (squash).

Security reports go privately as described in [SECURITY.md](SECURITY.md).
