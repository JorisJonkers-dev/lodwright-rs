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

Implemented, from synthetic fixtures: the plugin reader (groups, compressed records, `WRLD`/`CELL`/`LAND`), the LAND
height decoder, worldspace assembly across a load order, the block-aligned height raster, terrain LOD planning
(levels 4, 8, 16 and 32: blocks, decimated meshes, error, vertex cap, texture sizes, output paths) and the CLI
(`doctor`, `plan`, `heightmap`).

Not implemented, because it needs facts only the owner's real files can give: the `.btr`, `.bto` and `.lod`
writers, terrain textures and normal maps, object and tree LOD, BSA packing. What to run on real data and what to
report back: [docs/owner-runbook.md](docs/owner-runbook.md).

## Scope

Heightfield reader, block-aligned raster and terrain planning (done), then terrain meshes on disk, terrain
textures, object and tree LOD. Out of scope: running the game, and shipping or reading anyone's game data from this
repository.

## Where to read next

- [docs/owner-runbook.md](docs/owner-runbook.md): what the owner runs on real data, and what to report back.
- [docs/design.md](docs/design.md): pipeline, crates, first-slice interfaces, open questions.
- [docs/clean-room.md](docs/clean-room.md): who may read what; provenance rules; what CI enforces.
- [docs/TESTING.md](docs/TESTING.md): synthetic fixtures only, properties, oracle plan, `verify` design.
- [docs/PRACTICES.md](docs/PRACTICES.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).
