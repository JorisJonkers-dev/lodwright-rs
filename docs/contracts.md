# Contracts

lodwright emits JSON documents that other tools read. Each document is a contract and says so. lodwright owns these
Contracts: plan, request, result, manifest, verify and the progress events. Their published schemas are authoritative:
each is under `schemas/`, printed by `lodwright schema <name>` and published with every release.
Raul's Trusty Toolbox keeps the registry: which Contracts exist, which versions each Known-good set speaks, and a
snapshot of each such version (ADR-0009 in
[rauls-trusty-toolbox](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox/tree/main/docs/adr)). The Toolbox
(`raul`), not a mod manager, drives lodwright through them (ADR-0007 there).

Each document carries:

* `schema_version`: an integer. It rises when a key is removed, renamed or retyped, or when a value is added to an enum
  that is not open. A new optional key does not raise it, so a reader ignores keys it does not know.
* `contract`: the name of the contract, `<tool>-<noun>`. A reader refuses a document whose name is not the one it asked
  for. Today: `lodwright-plan` (`lodwright plan --json`), `lodwright-result` (`lodwright generate --json`, and the error
  document of a failed `--json` command), `lodwright-request` (read by `generate --request`), `lodwright-manifest`
  (`manifest.json` in the output folder) and `lodwright-verify` (`lodwright verify --json`). All are typed in
  `crates/lodwright-contract`; their schemas are under `schemas/` and require `contract` with the constant name.

The progress events (`lodwright-events`) are single JSON lines, not documents: they carry neither `schema_version` nor
`contract`. Their schema (`lodwright schema events`) describes one line, and the handshake gives their version.

## Handshake

`lodwright contracts --json` prints every contract this build emits and the schema versions it can emit, and exits 0:

```json
{"contracts":[{"name":"lodwright-plan","versions":[1]},{"name":"lodwright-result","versions":[1]},{"name":"lodwright-request","versions":[1]},{"name":"lodwright-manifest","versions":[1]},{"name":"lodwright-verify","versions":[1]},{"name":"lodwright-events","versions":[1]},{"name":"lodwright-api","versions":[1]}]}
```

`lodwright --version --json` lists the same contracts.

`lodwright-api` is not a document: it is the version of the in-process library API (`crates/lodwright-api`,
`lodwright_api::API_VERSION`). It rises when a call, a type or an error code of that API changes in a way an existing
caller would notice.

A consumer picks the highest version both sides list that is not above the version it was tested with, and refuses a
tool whose list lacks that version. This build emits only the newest version of each contract.

## Open enums

An enum in a published schema whose values may grow carries `"x-open-enum": true`, and a reader treats an unknown value
as "other". An enum without the mark is closed: adding a value to it is a version change.

## Request and manifest

`lodwright generate --request FILE` reads a `lodwright-request` document (schema: `lodwright schema request`): data
folder, plugins in load order, worlds (empty: all), output folder, terrain levels, object LOD on or off, inclusion rules
(exclude or keep by plugin, exclude by record or by model glob) and limits (threads, a memory budget). The request
digest (`arguments_digest` in the result) covers every field. The library entry point for the same work is
`lodwright_api::generate`, which takes the same request; the command is a wrapper over it.

### Adding a request option

The request digest is the `sha256` of the request's JSON as lodwright writes it, and the manifest keeps it to answer
`up_to_date`. A key that is written even when it holds its default would change the digest of every existing request,
so every output folder would be rebuilt once for no reason, and a settings round trip that writes defaults out would
never be `up_to_date`. So a key added to the request after release 0.4.2:

* is optional when read (`#[serde(default)]`), and its default keeps the output byte for byte as it was;
* is not written while it holds its default (`skip_serializing_if`: `Option::is_none`, `Vec::is_empty`,
  `std::ops::Not::not` for a switch that is off by default, or a comparison with the default value);
* goes into a terrain block's digest when it changes what that block's files hold.

The keys that existed at 0.4.2 are always written, as they were then; making them optional on output would change
the digest of every request. The test `the_default_request_keeps_its_released_digest`
(`crates/lodwright-contract/tests/contract.rs`) holds the digest of a default request at its released value and is
not rewritten by `UPDATE_GOLDEN`; the golden `request_default.json` next to it shows the JSON that digest covers. A
change that fails it is a change of every existing output folder's state, not a test to refresh.

A new optional key does not raise `schema_version`, but it does change `schemas/lodwright-request.v1.json`: the test
`committed_schemas_equal_the_generated_ones` fails until the schema file is regenerated (`UPDATE_GOLDEN=1`) and the
regenerated file is part of the same pull request. The Toolbox checks every request it builds against its snapshot of
that schema, so a new option can be used from the Toolbox only after a release and a Component bump that refreshes the
snapshot. Options land one pull request at a time, because each one edits the request type, the schema file and this
page.

The run leaves `manifest.json` (`lodwright-manifest`: request digest, inputs digest, every file with size and digest) in
the output folder. A later run with the same request over the same plugin contents finds it and the files it lists and
answers `up_to_date` without writing. The manifest also lists every terrain block with a digest of what its files
depend on (the generator version, the level settings, the water and land texture switches, the block's heights and,
per cell, the water height, the land layers and the pixels of the land textures they name). A run with a different
request or input keeps a block whose digest is unchanged and whose files still have the recorded digests, builds the
others, and removes the old manifest's files it neither kept nor wrote. The result's `blocks` counts the blocks built
(`generated`) and kept (`up_to_date`); each block event carries `up_to_date`. Object and tree LOD are rebuilt in full
whenever the run does not end as `up_to_date`. An output folder that is not empty and has no manifest is refused
(`output_not_empty`) unless `--overwrite` is given.

## Verify

`lodwright verify <folder> --json` prints a `lodwright-verify` document (schema: `lodwright schema verify`): `ok` (no
errors), how many terrain blocks, object blocks, tree blocks, textures and settings files were checked, `errors` (what is
wrong with the folder) and `differences` (how it differs from `--reference`, empty without one). Each finding is a path
relative to the folder and a sentence for people; callers match on the path and the counts, never on the sentence.
Nothing read from a file is copied into the report: only file names and numbers.

With `--reference`, the report also carries two optional keys, measured per block mesh present on both sides (names
compared in lower case):

* `water`: for every terrain block with water on either side, the IoU (intersection over union) of the ground the
  two water planes cover seen from above. It is sampled at the centres of a 400 by 400 grid over the bounds of both
  planes, the same definition the 0.3.0 Release test used, so numbers stay comparable; 0 when only one side has water.
  The summary gives the blocks, the `threshold`, how many are `under_threshold`, the `lowest` and the `mean`; every
  number is rounded to three decimals. `threshold` is `--water-iou` when given, otherwise 0.95 (the G5 goal), and only
  `--water-iou` turns a block under it into a difference (and so exit 1).
* `objects`: for every object block, the vertex and triangle counts on both sides and the number of vertices with no
  vertex of the reference's block within 0.05 units, in any order; `exact` when the counts are equal and that number is
  0. A block that is not exact is reported, never a difference: the Release test lists such blocks with a reason.

Exit codes of `verify`, the same in `lodwright --help` and `docs/TESTING.md`:

| Code | Meaning |
| --- | --- |
| 0 | clean: no error in the folder and, with `--reference`, no difference |
| 1 | findings: at least one error or difference; the report is still printed |
| 2 | usage error: no folder, an unknown option, an option without its value |
| 3 | unreadable folder: the folder or the reference is missing, not a folder, or cannot be listed; nothing is printed on stdout |

`ok` in the report says only that the folder has no errors; the exit code also counts the differences.
