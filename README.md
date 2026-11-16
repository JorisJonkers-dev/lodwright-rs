# lodwright-rs

A Linux-native, clean-room generator of terrain and object LOD (level of detail) data for Fallout: New Vegas and
Tale of Two Wastelands, written in Rust. The existing generators are Windows-only; this one is built from public
file-format documentation, our own measurements and synthetic tests.

**Licence: MPL-2.0.** How the clean-room rule and the licence follow from the licences of the existing tools is in
[docs/clean-room.md](docs/clean-room.md).

## Clean-room rule

Format facts come from public format documents, the MPL-2.0 xEdit source, public readmes, Microsoft's DDS
documentation, the game's own data formats on the owner's machine, and black-box output of an existing generator
run on the owner's own data. Decompiled or leaked code and unlicensed source are never read. Every fact the code
relies on is recorded in `provenance/sources.toml` and `provenance/LOG.md` (CI checks the record). Game data and
reference output never go in this repository; test fixtures are synthetic.

## Status

Working end to end for terrain and object LOD of a world: `lodwright generate` reads a load order and the data folder
(archives and loose files), and writes terrain block meshes, terrain diffuse and normal maps (land texture layers,
vertex colours, heights), the world settings file, object LOD blocks with their atlas pair, tree LOD (billboard atlas,
type list and block files, from billboard textures that mods ship), and a manifest. Output
matches the game's own tree in file names, counts and structure (see `docs/gap-list.md` for the measurements);
`lodwright verify` checks a folder against the formats and can compare it with another tree. A request document
(`lodwright schema request`) and the `lodwright-gen` library are the integration points for a mod manager.

Not done: water planes in terrain blocks, reuse of unchanged blocks inside a run, and loading in the game (owner only). The gap list has each item with its state.

## Scope

Heightfield reader, block-aligned raster and terrain planning (done), then terrain meshes on disk, terrain
textures, object and tree LOD. Out of scope: running the game, and shipping or reading anyone's game data from this
repository.

## Where to read next

- [docs/owner-runbook.md](docs/owner-runbook.md): what the owner runs on real data, and what to report back.
- [docs/gap-list.md](docs/gap-list.md): what full generation needs and what exists.
- [docs/LICENSING.md](docs/LICENSING.md): which crate carries which licence.
- [docs/design.md](docs/design.md): pipeline, crates, first-slice interfaces, open questions.
- [docs/clean-room.md](docs/clean-room.md): who may read what; provenance rules; what CI enforces.
- [docs/TESTING.md](docs/TESTING.md): synthetic fixtures only, properties, oracle plan, `verify` design.
- [docs/PRACTICES.md](docs/PRACTICES.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).
