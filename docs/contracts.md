# Contracts

lodwright emits JSON documents that other tools read. Each document is a contract and says so. lodwright owns these
Contracts: plan, request, result, manifest, verify and the progress events. Their published schemas are authoritative;
verify, manifest and events get their schema files and handshake entries in issue #87.
Raul's Trusty Toolbox keeps the registry: which Contracts exist, which versions each Known-good set speaks, and a
snapshot of each such version (ADR-0009 in
[rauls-trusty-toolbox](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox/tree/main/docs/adr)). The Toolbox
(`raul`), not a mod manager, drives lodwright through them (ADR-0007 there).

Each document carries:

* `schema_version`: an integer. It rises when a key is removed, renamed or retyped, or when a value is added to an enum
  that is not open. A new optional key does not raise it, so a reader ignores keys it does not know.
* `contract`: the name of the contract, `<tool>-<noun>`. A reader refuses a document whose name is not the one it asked
  for. Today: `lodwright-plan` (`lodwright plan --json`) and `lodwright-result` (`lodwright generate --json`, and the error
  document of a failed `--json` command). Both are typed in `crates/lodwright-contract`; their schemas are under
  `schemas/` and require `contract` with the constant name.

## Handshake

`lodwright contracts --json` prints every contract this build emits and the schema versions it can emit, and exits 0:

```json
{"contracts":[{"name":"lodwright-plan","versions":[1]},{"name":"lodwright-result","versions":[1]},{"name":"lodwright-request","versions":[1]},{"name":"lodwright-api","versions":[1]}]}
```

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

## Progress events

`lodwright generate --events FILE` (either form of the command; `-` for stderr) writes one JSON line per finished
block, in the order the blocks finish:

```json
{"event":"block","kind":"objects","world":"Island","level":4,"done":1,"of":2,"up_to_date":false}
```

* `kind` is `terrain`, `objects` or `trees` (an open enum). A line without `kind` was written before the key existed
  and is a terrain block.
* `level` is the number of cells along a block edge: the terrain level, 4 for an object block, 8 for a tree block.
* `done` and `of` count the blocks of that kind and level in that world; `of` is known before the first of them
  finishes, and `done` reaches it. Object blocks count each block file, so a block with high-priority objects counts
  twice (its normal and its `high` file).
* `up_to_date` is true for a terrain block kept from an earlier run; object and tree blocks are always rebuilt, so
  it is false for them.

Per world, the terrain lines come first (level by level), then the object lines, then the tree lines. The terrain
blocks of a level finish on several workers, so with more than one worker their lines may arrive with `done` out of
order; a reader that shows progress keeps the highest `done` it has seen. Object and tree lines come in order.

The library reports the same thing through its progress callback (`lodwright_api::Progress`): each step starts with
a `ProgressEvent` without counts, and each finished block is a `ProgressEvent` with `step` (`terrain`, `objects` or
`trees`), `world`, `level`, `done` and `total`. `ProgressEvent::block_line` turns such an event into the line above,
and the command writes exactly those lines, so the file and the callback cannot disagree. A caller that maps the
callback, as the Toolbox does, should:

* treat an event with `done` and `total` as progress of its step (`label_key`: `lod.terrain`, `lod.objects`,
  `lod.trees`), and every other event as the start of its step;
* word `level` by step: a terrain level for `lod.terrain`, nothing user-facing for object and tree blocks (it is
  the block size, not a quality level);
* treat a step it does not know as the start of an unnamed step, not as an error.

Object and tree block events were added without changing a type, a field or an error code of the library, so
`lodwright-api` stays at version 1: a caller built for the earlier events keeps compiling and sees more progress
events.
