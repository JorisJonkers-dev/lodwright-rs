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

## Release assets and the contract versions

Every release carries these assets, built on Linux x86_64 by `release.yml` (`scripts/release-assets.sh`) and listed in
`SHA256SUMS`. A build provenance attestation covers all of them (`gh attestation verify <file> --repo
JorisJonkers-dev/lodwright-rs`).

| Asset | What |
| --- | --- |
| `lodwright-x86_64-unknown-linux-gnu` | the binary |
| `lodwright-universal-apple-darwin` | the macOS binary, one file for Apple silicon and Intel (built on macOS, joined with `lipo`); not signed yet, so macOS asks before the first run |
| `lodwright-plan.v1.json` | JSON Schema of the plan document (`lodwright plan --json`) |
| `lodwright-result.v1.json` | JSON Schema of the result and error document (`lodwright generate --json`) |
| `lodwright-request.v1.json` | JSON Schema of the request document (`lodwright generate --request`) |
| `SHA256SUMS` | SHA-256 of every asset above, in `sha256sum` format |

The schemas are generated from the types of `crates/lodwright-contract` and committed under `schemas/`; a test fails
when they drift, and the release build fails when the binary prints a different schema than the committed file.
`lodwright --version --json` prints the contract versions a binary speaks.

| Release | Plan | Result | Library API |
| --- | --- | --- | --- |
| first release with the contract (after 0.1.0) | 1 | 1 | |
| first release with `crates/lodwright-api` (after 0.3.0) | 1 | 1 | 1 |

The library API (`lodwright_api::API_VERSION`, listed as `lodwright-api` by `lodwright --version --json`) is versioned
with each release in the same way: a caller links a tagged release, and a change an existing caller would notice
raises the version and gets a row here.

A change to a document that adds an optional field is not a version bump; readers ignore unknown fields. A breaking
change adds a `v2` schema beside `v1`, keeps printing `v1` on `--schema-version 1` for at least one Known-good set of Raul's Trusty Toolbox, and
gets a row here. The release procedure above is unchanged: the asset names carry the document version, never the tool
version.

A dry run builds and attests the same assets without a release: run the Release workflow by hand
(`gh workflow run release.yml --ref <branch>`); the assets stay as the workflow artifact `release-assets-dry-run`.
