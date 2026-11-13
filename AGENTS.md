# Agents

Guidance for any coding agent working in this repository.

- Agent contract: [CLAUDE.md](CLAUDE.md)
- Estate conventions: https://github.com/JorisJonkers-dev/workspace/blob/main/CLAUDE.md

lodwright-rs is a clean-room Rust terrain and object LOD generator. Read [docs/clean-room.md](docs/clean-room.md)
before touching a format or reading any source of an existing generator: the clean-room rule decides
which inputs are allowed.

## Build and test

- Everything CI runs: `task check`
- Apply the repository settings and ruleset: `scripts/setup-repo.sh` (owner only)
