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
  (`manifest.json` in the output folder), `lodwright-verify` (`lodwright verify --json`), `lodwright-defaults`
  (`lodwright defaults --json`) and `lodwright-bands` (`lodwright bands --json`). All are typed in
  `crates/lodwright-contract`; their schemas are under `schemas/` and require `contract` with the constant name.

The progress events (`lodwright-events`) are single JSON lines, not documents: they carry neither `schema_version` nor
`contract`. Their schema (`lodwright schema events`) describes one line, and the handshake gives their version.

## Handshake

`lodwright contracts --json` prints every contract this build emits and the schema versions it can emit, and exits 0:

```json
{"contracts":[{"name":"lodwright-plan","versions":[1]},{"name":"lodwright-result","versions":[1]},{"name":"lodwright-request","versions":[1]},{"name":"lodwright-manifest","versions":[1]},{"name":"lodwright-verify","versions":[1]},{"name":"lodwright-events","versions":[1]},{"name":"lodwright-defaults","versions":[1]},{"name":"lodwright-bands","versions":[1]},{"name":"lodwright-api","versions":[1]}]}
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

### Memory budget

`limits.memory_mib` is checked twice. Before anything is read, a run whose plugins alone suggest more (four times their
file sizes) is refused as `memory_limit`. While the run goes, each world's terrain step gets as many workers as the
budget leaves room for: the limit, minus what the process holds at that moment (with the world's land textures loaded),
minus the world's height raster, divided by what one worker needs: the largest block textures of the request (each with
its mip chain and encoded file) plus 8 MiB for the block mesh and its planner. It never drops below one worker, and a
result warning names each world that ran with fewer workers than asked for. The worker count never changes a byte of the
output, so a run under a budget writes the same files as one without.

The resident size is read on Linux (`VmRSS`), where the result also reports the peak (`peak_memory_mib`). On Windows
and macOS the preflight estimate stands in for it, so the budget is measured on Linux and best effort elsewhere. Object
and tree LOD run on one thread per world and are not scaled. The budget is a target, not a hard cap: a single worker
that needs more than is left still runs. The Toolbox labels it advisory.

### Request defaults

`lodwright defaults --json` and `lodwright_api::request_defaults()` give the same `lodwright-defaults` document (schema:
`lodwright schema defaults`): the terrain `levels` a request may list, and every option a user can set, in request
order. Each option has its dotted `key` in the request (`terrain.water`; `terrain.levels.quality` for a per-level
setting), its `kind` (`switch`, `integer`, `number`, `list`; an open enum), its `default` (`null` where the default is
"the machine decides" or "no limit"), for a per-level option its `per_level` defaults, the `minimum`, `maximum` and
`power_of_two` that `Request::validate` checks, its `tier` (1: every user; 2: per-level terrain settings; 3: an option a
later release added, behind the advanced gate) and `since`, the release that introduced it. The defaults and ranges
are read from the code that applies them (the request's `Default`, the terrain settings of `lodwright-core` and the
constants `validate` uses), and a test fails when the table and that code disagree.

A request lists a terrain level whole: to change one setting of a level, a caller writes that level with the other
settings at their `per_level` defaults, and leaves `terrain.levels` empty while nothing differs from the defaults (so
the request digest stays the default one).

A consumer that links an older lodwright without this call, or reads a handshake without `lodwright-defaults`, shows
no default label and offers no tier 2 or tier 3 row. Adding the call changed no existing call, type or error code, so
`lodwright-api` stays at version 1.

### Land texture tiling

`terrain.texture_scales` (empty by default, written only when set) says how often a land texture repeats across a
cell quadrant in the terrain diffuse maps, by the land texture's absolute `FormID` as eight hex digits, for example
`{"0000ABCD": 2.0}`; a texture that is not listed repeats once, as before. Each scale lies between 0.0625 and 16
(`TEXTURE_SCALE_RANGE`). No source is known for a per-texture scale in the game's records, so this is lodwright's own
setting, not a value read from the plugins. A scale other than 1 enters the digest of every terrain block that uses the
texture, so changing it rebuilds those blocks and keeps the others; with no scale set, block digests and output are the
same as before the option existed.

### Object faces under the ground and the water

`objects.drop_under_terrain` and `objects.drop_under_water` (both off by default, written only when on) leave out
object LOD faces that cannot be seen: a face whose three corners all lie below the ground at their positions (the
cell's height samples, interpolated between the four around each corner), or below the water height of a cell that has
water. A face that crosses the surface is kept. Vertices that no kept face uses are left out too, so the UVs of the
others stay where they were in the atlas. This is lodwright's own design, not a rule of the game's files. With either
option on, the result counts the faces left out (`dropped_faces`: `under_terrain`, `under_water`; a face under both
counts as under the ground). With both off, the output and the request digest are the same as before the options
existed.

### Adding a request option

The request digest is the `sha256` of the request's JSON as lodwright writes it, and the manifest keeps it to answer
`up_to_date`. A key that is written even when it holds its default would change the digest of every existing request,
so every output folder would be rebuilt once for no reason, and a settings round trip that writes defaults out would
never be `up_to_date`. So a key added to the request after release 0.4.2:

* is optional when read (`#[serde(default)]`), and its default keeps the output byte for byte as it was;
* is not written while it holds its default (`skip_serializing_if`: `Option::is_none`, `Vec::is_empty`,
  `std::ops::Not::not` for a switch that is off by default, or a comparison with the default value);
* goes into a terrain block's digest when it changes what that block's files hold;
* gets a row in `request_defaults()` (`crates/lodwright-api/src/defaults.rs`) with its tier and the release that
  introduced it.

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

Nothing is written, replaced or removed outside the output folder. The folder may itself be a link: it is refused
when it points into the data folder, and when either the link or the folder it points to lies in a git working tree. Below it, a folder on a file's path that is a link is
refused (`output_link`, nothing is written through it). A temporary file is always created new, so a link already at
its path is removed, never followed. A manifest that lists an absolute path or one with `..` is not trusted and is
treated as no manifest.

## Verify

`lodwright verify <folder> --json` prints a `lodwright-verify` document (schema: `lodwright schema verify`): `ok` (no
errors), how many terrain blocks, object blocks, tree blocks, textures and settings files were checked, `errors` (what is
wrong with the folder) and `differences` (how it differs from `--reference`, empty without one). Each finding is a path
relative to the folder and a sentence for people; callers match on the path and the counts, never on the sentence.
Nothing read from a file is copied into the report: only file names and numbers.

With `--reference`, the report also carries three optional keys, measured per block mesh present on both sides (names
compared in lower case):

* `water`: for every terrain block with water on either side, the IoU (intersection over union) of the ground the
  two water planes cover seen from above. It is sampled at the centres of a 400 by 400 grid over the bounds of both
  planes, the same definition the 0.3.0 Release test used, so numbers stay comparable; 0 when only one side has water.
  The summary gives the blocks, the `threshold`, how many are `under_threshold`, the `lowest` and the `mean`; every
  measured number is rounded to three decimals. `threshold` is `--water-iou` when given, otherwise 0.95 (the G5 goal), and only
  `--water-iou` turns a block under it into a difference (and so exit 1).
* `terrain`: for every terrain block mesh present on both sides, how far the ground heights differ. The ground of both
  meshes (the first shape of the block) is sampled on a 33 by 33 grid over the bounds of the block's mesh, at the points
  both meshes cover, and the absolute height difference is taken in game units: per block the `max` and the `mean`, and
  in the summary the blocks compared, the `tolerance`, how many blocks are `over_tolerance`, the largest `max` and the
  mean of the means, plus `per_level` like the water summary. Numbers are rounded to three decimals. `tolerance` is
  `--height-tolerance U` when given, otherwise 1 unit, and only `--height-tolerance` turns a block over it into a
  difference (and so exit 1). A block pair that shares no sampled point is left out. Versions of this document stay at 1:
  the key is optional and readers ignore keys they do not know.
* `objects`: for every object block, the vertex and triangle counts on both sides and the number of vertices with no
  vertex of the reference's block within 0.05 units, in any order; `exact` when the counts are equal and that number is
  0. A block that is not exact is reported, never a difference: the Release test lists such blocks with a reason.

With `--reference`, `water.per_level` repeats the water summary per terrain level (blocks, how many are under the
threshold, lowest and mean), smallest level first; the text report prints one `water level L:` line each. These are the
numbers a release lists per level when it ships with known differences.

`--known-differences FILE` (only with `--reference`) reads a list of accepted differences. One entry per line, three
fields separated by tabs: the block path as `verify` prints it, the release expected to remove the difference, and the
reason; blank lines and lines starting with `#` are ignored, and paths compare in lower case:

```text
# path	release	reason
meshes/landscape/lod/wastelandnv/wastelandnv.level4.x-8.y12.nif	0.7.0	water outline not built yet (#151)
```

A difference whose path the list names moves from `differences` to `known` (with its problem, the reason and the
release) and no longer makes the run exit 1; a difference the list does not name still does. A listed path that no
longer differs is reported in `stale`, so the list does not grow forever. A list that cannot be read or has a line
without its three fields is a usage error (exit 2) naming the line. The list holds block paths and words only, never
data from the game.

Exit codes of `verify`, the same in `lodwright --help` and `docs/TESTING.md`:

| Code | Meaning |
| --- | --- |
| 0 | clean: no error in the folder and, with `--reference`, no difference |
| 1 | findings: at least one error or difference; the report is still printed |
| 2 | usage error: no folder, an unknown option, an option without its value, or a folder or reference inside a git working tree without `--allow-in-repo` (nothing is read) |
| 3 | unreadable folder: the folder or the reference is missing, not a folder, or cannot be listed; nothing is printed on stdout |

`ok` in the report says only that the folder has no errors; the exit code also counts the differences.

### Bands

`lodwright bands <verify.json> ... [--json]` reads the `verify --json` reports of several runs of the same inputs and
prints, per statistic, the minimum, the maximum and how many runs report it (`lodwright-bands`, schema: `lodwright
schema bands`). The statistics are the report's counts (`terrain_blocks`, `errors`, `differences` and so on),
`water.blocks`, `water.under_threshold`, `water.lowest`, `water.mean`, `terrain.blocks`, `terrain.over_tolerance`,
`terrain.max`, `terrain.mean`, `terrain.max:<path>` per block, `objects.blocks`, `objects.exact`, and per
block `water.iou:<path>` and `objects.unmatched_vertices:<path>`. Every value is rounded to three decimals first, so the
order of float operations never widens a band; a band whose minimum equals its maximum is an exact match. A file that
is not a verify report is a failure (exit 1); no file is a usage error (exit 2).

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
  finishes, and `done` reaches it. Object blocks count each block file the world's objects fall into, so a block
  with high-priority objects counts twice (its normal and its `high` file); a block whose objects all turn out to be
  left out (for example over the vertex limit, with a warning) still counts, without a file.
* A reader that meets a `kind` it does not know treats the line as progress of an unnamed kind (the Rust type reads
  it as `BlockKind::Other`).
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
