# Versioning and release

lodwright-rs is versioned with SemVer and released by release-please (`release-type: simple`, see
`docs/PRACTICES.md` for why). Nothing is published to crates.io or any package registry.

1. Land changes through squash-merged pull requests with Conventional Commit titles
   (`feat:`, `fix:`, `chore:`, `feat!:` for breaking changes).
2. `release.yml` keeps a release pull request open. Merging it tags `vX.Y.Z`, writes `CHANGELOG.md` and bumps
   `.release-please-manifest.json`.
3. Pre-1.0 (`0.y.z`): a minor bump is the breaking lever (`bump-minor-pre-major`).

Dependencies, including GitHub Actions, are pinned exactly (actions by commit SHA); Renovate proposes the bumps and
every bump must pass `Pipeline Complete`.
