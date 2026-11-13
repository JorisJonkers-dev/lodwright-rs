# xlodgen-rs

A Rust port of [xLODGen](https://github.com/sheson/xLODGen) (terrain, object and tree LOD generation for
Fallout: New Vegas / Tale of Two Wastelands, by sheson, built on [xEdit](https://github.com/TES5Edit/TES5Edit))
that runs natively on Linux. The original is Windows-only.

**Licence: MPL-2.0**, the licence of xEdit, whose source xLODGen's host code is part of
(`Core/wbLOD.pas` and neighbours in TES5Edit/TES5Edit, read on 2026-11-13). Files translated from those
sources stay MPL-2.0.

## Licence reasoning

- Open: the xEdit side of xLODGen (LOD settings, tree LOD lists, atlas and bin packing, the drivers) is
  MPL-2.0 and may be ported.
- Closed: the terrain LOD generator and `LODGen.exe` 3.x have no published source or licence. The only public
  `LODGen` source (TES5Edit/xLODGen, 2016) has no licence and is therefore all rights reserved.
- So the terrain and object engines are a **clean-room reimplementation**, from file formats and observed
  behaviour only.

## Clean-room rule

Allowed inputs: format documents (UESP and similar), xEdit's MPL source, public readmes and guides, Microsoft
DDS documentation, the game's own data formats, and outputs of xLODGen run on your own data. Not allowed:
decompiled or disassembled xLODGen/LODGen binaries, the unlicensed LODGen C# source, leaked or pasted source.
Whoever implements the engines works from the recorded facts (`docs/formats/` when it exists) and the MPL sources, not from the closed code. Record
where each format fact came from in `provenance/sources.toml` and `provenance/LOG.md` (CI checks the record). Game data and oracle outputs never go in this
repository; test fixtures are synthetic.

## Status

Setup only: a Cargo workspace (`xlodgen-core`, `xlodgen-cli`, `xlodgen-testkit`), the pipeline and the clean-room
guardrails. **No LOD generation is implemented and no game format is parsed**; the testkit builds synthetic bytes for
tests. The CLI has `--version`, `--help` and `doctor <data-folder>` (checks the folder exists, prints nothing from
it).

## Scope

Planned: LAND/VHGT heightfield reader, `.lod` writer and a block-aligned raster (first slice), then `.btr`, terrain
textures, object and tree LOD. Linux-native; the original is Windows-only. Out of scope: running the game, and
shipping or reading anyone's game data from this repository.

## Where to read next

- [docs/design.md](docs/design.md): pipeline, crates, first-slice interfaces, open questions.
- [docs/clean-room.md](docs/clean-room.md): who may read what; provenance rules; what CI enforces.
- [docs/TESTING.md](docs/TESTING.md): synthetic fixtures only, properties, oracle plan, `verify` design.
- [docs/PRACTICES.md](docs/PRACTICES.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).

Not affiliated with sheson or the xEdit team. No one has been contacted about this port.
