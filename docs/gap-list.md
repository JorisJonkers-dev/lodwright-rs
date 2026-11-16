# Gap list: what full LOD generation needs and what exists

State on 2026-11-16, after the measurements in `provenance/LOG.md` (entry of that date). "Needs" is the whole
pipeline the community generators provide for this game. Each row is one work item; the order is the build order.

| # | Part | State | What is missing |
| --- | --- | --- | --- |
| 1 | Plugin reading, load order, master order | Done (own reader, header checks through the `esplugin` crate) | Compressed `CELL` coverage on real files is checked by the real-data run |
| 2 | Archives, loose-over-archive lookup | Done (`lodwright-assets`) | Wiring into the generator for textures and models |
| 3 | Worldspace assembly, height raster | Done | Parent-world inheritance is decoded; its use in LOD is untested on real data |
| 4 | Terrain mesh planning | Done (own decimation) | Border protection between blocks of different steps, hiding unseen faces, water offset |
| 5 | Terrain block NIF | Structure measured, writer missing | Writer for the eight-block layout, the per-vertex float channel, bounds |
| 6 | Terrain textures | Layer decoding done; composition missing | Layer blending into the diffuse, normal maps from heights, BC1 encoding at the measured sizes |
| 7 | Settings file | Writer and parser done (`lodwright-core::lod`) | Bounds taken from the world; check against the game's loading |
| 8 | Output layout | Done, matches the game's own tree | Atomic writes, manifest |
| 9 | Object LOD | Layout measured, nothing built | Selecting references (base record flag, high-priority flag), decoding `STAT`, `SCOL`, `REFR`, loading small LOD models from archives, merging per block, removing faces under terrain, atlas packing with UV remap, writing the block NIFs and the atlas DDS pair |
| 10 | Tree and billboard LOD | Not measured | `.dtl` and `treetypes.lst` layouts, tree record decoding, the fixed atlas |
| 11 | Inclusion rules | Not built | Per plugin and per record include and exclude lists, reading them from a request file |
| 12 | Determinism, incremental runs | Terrain ready to build on | Stable ordering everywhere, content hashes per block, a manifest and reuse of unchanged blocks |
| 13 | Performance | Not built | Parallel blocks with a bounded worker count, a memory cap, streaming of large plugins |
| 14 | CLI | `plan`, `heightmap`, `probe`, stub `generate` | A real `generate`, clear error codes, `verify` |
| 15 | Integration for the mod manager | Contracts exist (plan, result) | A request document, progress events, stable exit codes, a library entry point that does not print |
| 16 | In-game acceptance | Owner only | Loading generated blocks in the game |

What verification means here: tests build synthetic plugins and archives; real-data checks read the owner's install
read-only, write to a scratch folder, and report numbers only.
