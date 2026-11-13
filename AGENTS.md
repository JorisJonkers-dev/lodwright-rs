# Agents

Guidance for any coding agent working in this repository.

- Agent contract: [CLAUDE.md](CLAUDE.md)
- Estate conventions: https://github.com/JorisJonkers-dev/workspace/blob/main/CLAUDE.md

xlodgen-rs is a planned clean-room Rust port of xLODGen. Read [docs/clean-room.md](docs/clean-room.md) (written
in a following pull request) before touching a format or reading any xLODGen source: the clean-room rule decides
which inputs are allowed.

## Build and test

- Everything CI runs: `task check` (arrives with the Cargo workspace)
- Apply the repository settings and ruleset: `scripts/setup-repo.sh` (owner only)
