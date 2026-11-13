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

A Cargo workspace (`lodwright-core`, `lodwright-cli`, `lodwright-testkit`), the pipeline and the clean-room
guardrails. The CLI binary is `lodwright`.

## Scope

Heightfield reader, `.lod` writer and a block-aligned raster (first slice), then terrain meshes, terrain textures,
object and tree LOD. Out of scope: running the game, and shipping or reading anyone's game data from this
repository.

## Where to read next

- [docs/design.md](docs/design.md): pipeline, crates, first-slice interfaces, open questions.
- [docs/clean-room.md](docs/clean-room.md): who may read what; provenance rules; what CI enforces.
- [docs/TESTING.md](docs/TESTING.md): synthetic fixtures only, properties, oracle plan, `verify` design.
- [docs/PRACTICES.md](docs/PRACTICES.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).
